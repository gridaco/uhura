use std::str::FromStr;
use uhura_check::check_v04_module;

use uhura_core::{Decimal, ReactionResolution, Value};
use uhura_syntax::v04::{SourceIdentity, parse};

fn check(source: &str) -> uhura_check::CheckOutput {
    let parsed = parse(
        SourceIdentity::new(71, "example.standard@1", "standard", "standard.uhura"),
        source,
    );
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    check_v04_module(&parsed.module)
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

#[test]
fn standard_imports_routes_literal_maps_and_empty_sets_reach_the_kernel() {
    let output = check(
        r#"
use uhura::boundary::Token;
use uhura::observation::Observation;
use uhura::ports::{RequestPort, SinkPort};
use uhura::ui_surface::Surface;
use uhura::web_router::{Link, Router, Routes};

pub key RequestId(PositiveInt);
pub key ItemId(Text);

pub enum Location {
  Home,
  Item { id: ItemId },
}

pub enum Request {
  Load,
}

pub enum Reply {
  Loaded,
}

pub const ROUTES: Routes<Location> = Routes::from([
  ("Home", "/"),
  ("Item", "/items/{id}"),
]);

pub const ITEM: ItemId = ItemId("first");
pub const ITEMS: Map<ItemId, Text> = Map::from([
  (ITEM, "First"),
]);
pub const EMPTY: Set<ItemId> = Set::empty();

pub machine StandardProbe {
  port router = Router<Location> { routes: ROUTES };
  port observation = Observation<Text> {};
  port requests = RequestPort<RequestId, Request, Reply> {};
  port sink = SinkPort<Text> {};

  outcomes {
    commit Done,
  }

  on router.Changed(location) {
    Done
  }

  on observation.Observed(value) {
    Done
  }

  on requests.Settled(id, result) {
    Done
  }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked standard-library probe");
    assert_eq!(
        program.constants["example.standard@1::ITEMS"],
        Value::Map(vec![(
            Value::Key {
                type_id: "example.standard@1::ItemId".into(),
                value: Box::new(Value::Text("first".into())),
            },
            Value::Text("First".into()),
        )])
    );
    assert_eq!(
        program.constants["example.standard@1::EMPTY"],
        Value::Set(Vec::new())
    );
    assert!(
        program
            .route_tables
            .contains_key("example.standard@1::ROUTES")
    );
    assert_eq!(
        program.machines["example.standard@1::StandardProbe"]
            .ports
            .len(),
        4
    );
}

#[test]
fn standard_aliases_are_lexical_and_unknown_exports_are_rejected() {
    let aliased = check(
        r#"
use uhura::ports::SinkPort as Output;

pub machine AliasProbe {
  port output = Output<Text> {};
  outcomes { commit Done }
}
"#,
    );
    assert!(
        aliased.diagnostics.is_empty(),
        "alias diagnostics:\n{:#?}",
        aliased.diagnostics
    );

    let unknown = check(
        r#"
use uhura::ports::Mystery;

pub machine UnknownProbe {
  outcomes { commit Done }
}
"#,
    );
    assert!(unknown.program.is_none());
    assert!(unknown.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura-0.4/unknown-standard-export"
            && diagnostic.message.contains("uhura::ports::Mystery")
    }));
}

#[test]
fn global_constants_are_evaluated_by_dependency_not_source_order() {
    let output = check(
        r#"
pub key ItemId(Text);

pub const A_MAP: Map<ItemId, Text> = Map::from([
  (Z_KEY, Z_LABEL),
]);
pub const Z_LABEL: Text = "late";
pub const Z_KEY: ItemId = ItemId("key");

pub machine ConstProbe {
  outcomes { commit Done }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("dependency-ordered constants");
    assert_eq!(
        program.constants["example.standard@1::A_MAP"],
        Value::Map(vec![(
            Value::Key {
                type_id: "example.standard@1::ItemId".into(),
                value: Box::new(Value::Text("key".into())),
            },
            Value::Text("late".into()),
        )])
    );
}

#[test]
fn generic_option_collection_helpers_accept_computed_sources_and_pure_binders() {
    let output = check(
        r#"
fn compact(values: Seq<Option<Int>>) -> Seq<Int> {
  Seq::from_options(values)
}

fn selected(values: Seq<Int>) -> Set<Int> {
  Set::filter_map(values, |value|
    if value > 1 { Some(value * 10) } else { None }
  )
}

fn selected_keys(values: Map<Text, Int>) -> Set<Text> {
  Set::filter_map(values.entries(), |entry|
    if entry.value > 1 { Some(entry.key) } else { None }
  )
}

pub machine CollectionProbe {
  state {
    options: Seq<Option<Int>> = [Some(2), None, Some(1), Some(2)],
    values: Map<Text, Int> = Map::from([("b", 1), ("a", 2)]),
  }
  computed compacted: Seq<Int> = compact(options);
  computed selected_values: Set<Int> = selected(compact(options));
  computed keys: Set<Text> = selected_keys(values);
  observe { compacted, selected_values, keys }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "collection diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("generic collection helper program");
    let (instance, _) = program
        .admit(
            "example.standard@1::CollectionProbe",
            Value::Unit,
            "standard/generic-collections",
        )
        .expect("generic collection admission");
    let Value::Record(observation) = instance.observation else {
        panic!("expected record observation")
    };
    assert_eq!(
        observation[0].1,
        Value::Seq(vec![Value::int(2), Value::int(1), Value::int(2)])
    );
    assert_eq!(observation[1].1, Value::Set(vec![Value::int(20)]));
    assert_eq!(observation[2].1, Value::Set(vec![Value::Text("a".into())]));
}

#[test]
fn set_add_is_idempotent_for_key_and_named_values_across_reactions() {
    let output = check(
        r#"
pub key ItemId(Text);

pub enum Marker {
  Selected,
}

pub machine SetProbe {
  events {
    Add(item: ItemId),
  }
  outcomes {
    commit Done,
  }
  state {
    items: Set<ItemId> = Set::empty(),
    markers: Set<Marker> = Set::empty(),
  }
  observe { items, markers }

  on Add(item) {
    items = items.add(item).add(item);
    markers = markers.add(Marker::Selected).add(Marker::Selected);
    Done
  }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "set diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked set probe");
    let machine = "example.standard@1::SetProbe";
    let item = Value::Key {
        type_id: "example.standard@1::ItemId".into(),
        value: Box::new(Value::Text("same".into())),
    };
    let add = Value::variant(
        format!("{machine}.Input"),
        "Add",
        vec![(Some("item".into()), item.clone())],
    );
    let (instance, _) = program
        .admit(machine, Value::Unit, "standard/set-add")
        .expect("set probe admission");
    let first = program.react(&instance, add.clone()).expect("first add");
    let second = program.react(&first.instance, add).expect("repeated add");
    for result in [&first, &second] {
        assert!(matches!(
            result.receipt.resolution,
            ReactionResolution::Completed { .. }
        ));
        assert_eq!(
            record_field(&result.instance.observation, "items"),
            &Value::Set(vec![item.clone()])
        );
        assert_eq!(
            record_field(&result.instance.observation, "markers"),
            &Value::Set(vec![Value::variant(
                "example.standard@1::Marker",
                "Selected",
                Vec::new(),
            )])
        );
    }
}

#[test]
fn required_collection_prelude_preserves_sequence_order_and_entry_fields() {
    let output = check(
        r#"
pub machine PreludeProbe {
  state {
    items: Seq<Int> = [3, 1, 2, 3],
    values: Map<Text, Int> = Map::from([("b", 1), ("a", 2)]),
  }
  computed filtered: Seq<Int> = items.filter(|item| item > 1);
  computed matching: Nat = items.count(|item| item > 1);
  computed has_entry: Bool = values.entries().any(|entry|
    entry.key == "a" && entry.value == 2
  );
  observe { filtered, matching, has_entry }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "prelude diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked prelude inventory");
    let (instance, _) = program
        .admit(
            "example.standard@1::PreludeProbe",
            Value::Unit,
            "standard/prelude",
        )
        .expect("prelude admission");
    assert_eq!(
        record_field(&instance.observation, "filtered"),
        &Value::Seq(vec![Value::int(3), Value::int(2), Value::int(3)])
    );
    assert_eq!(
        record_field(&instance.observation, "matching"),
        &Value::nat(3).expect("Nat")
    );
    assert_eq!(
        record_field(&instance.observation, "has_entry"),
        &Value::Bool(true)
    );

    let invalid =
        check("fn invalid(values: Seq<Int>) -> Seq<Int> { values.filter(|value| value) }");
    assert!(invalid.program.is_none());
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "uhura/type-mismatch"),
        "non-Bool predicates must be rejected before runtime: {:?}",
        invalid.diagnostics
    );
}

#[test]
fn finite_views_remain_usable_as_ephemeral_evaluator_inputs() {
    let output = check(
        r#"
fn positive_count(values: FiniteView<Int>) -> Nat {
  values.count(|value| value > 0)
}

pub machine EphemeralViewProbe {
  state {
    values: Map<Text, Int> = Map::from([
      ("negative", -1),
      ("positive", 2),
      ("zero", 0),
    ]),
  }

  computed positives: Nat = positive_count(values.values());
  observe { positives }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "ephemeral FiniteView diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked ephemeral FiniteView probe");
    let (instance, _) = program
        .admit(
            "example.standard@1::EphemeralViewProbe",
            Value::Unit,
            "standard/ephemeral-finite-view",
        )
        .expect("ephemeral FiniteView admission");
    assert_eq!(
        record_field(&instance.observation, "positives"),
        &Value::nat(1).expect("Nat"),
    );
}

#[test]
fn finite_views_are_rejected_recursively_from_persisted_and_observable_boundaries() {
    let output = check(
        r#"
use uhura::ports::SinkPort;

pub struct WrappedView {
  values: FiniteView<Int>,
}

pub key InvalidViewKey(FiniteView<Int>);

pub const VALUES: Map<Text, Int> = Map::from([("one", 1)]);
pub const STORED_VIEW: Option<FiniteView<Int>> = None;

pub machine InvalidViewBoundaries {
  config {
    configured: FiniteView<Int>,
  }

  port output = SinkPort<FiniteView<Int>> {};

  events {
    Leak(value: FiniteView<Int>),
  }

  commands {
    Send(value: FiniteView<Int>),
  }

  outcomes {
    commit Done(value: FiniteView<Int>),
  }

  state {
    cached: Option<WrappedView> = None,
  }

  observe {
    leaked: VALUES.values(),
  }

  on Leak(value) {
    emit Send(value);
    Done(value)
  }
}
"#,
    );
    assert!(output.program.is_none());
    let boundaries = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == "uhura-0.4/ephemeral-finite-view")
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    for boundary in [
        "key `InvalidViewKey`",
        "constant `STORED_VIEW`",
        "machine configuration field `configured`",
        "port `output` contract",
        "input constructor `Leak`",
        "command constructor `Send`",
        "outcome constructor `Done`",
        "state field `cached`",
        "observation field `leaked`",
    ] {
        assert!(
            boundaries.iter().any(|message| message.contains(boundary)),
            "missing `{boundary}` rejection in:\n{:#?}",
            output.diagnostics,
        );
    }
    assert!(
        boundaries.iter().any(|message| {
            message.contains("state field `cached`")
                && message
                    .contains("nested path: `Option.value -> WrappedView.values -> FiniteView`")
        }),
        "nested wrappers must identify the complete FiniteView path:\n{:#?}",
        output.diagnostics,
    );
}

#[test]
fn a0_total_helpers_have_executable_success_and_refusal_semantics() {
    let output = check(
        r#"
fn increment(values: Seq<Int>) -> Option<Seq<Int>> {
  values.try_map(|value|
    if value >= 0 { Some(value + 1) } else { None }
  )
}

fn increment_values(values: Map<Text, Int>) -> Option<Map<Text, Int>> {
  values.try_map_values(|entry|
    if entry.value >= 0 { Some(entry.value + 1) } else { None }
  )
}

pub machine A0Helpers {
  state {
    boundary: BoundaryNumber = 2,
    fractional_boundary: BoundaryNumber = 0.5,
    values: Seq<Int> = [1, 2],
    invalid_values: Seq<Int> = [1, -1],
    pairs: Seq<(Text, Int)> = [("b", 2), ("a", 1)],
    duplicate_pairs: Seq<(Text, Int)> = [("a", 1), ("a", 2)],
    duplicates: Seq<Int> = [1, 1],
    unique_values: Seq<Int> = [2, 1],
    map: Map<Text, Int> = Map::from([("b", 2), ("a", 1)]),
    invalid_map: Map<Text, Int> = Map::from([("ok", 1), ("invalid", -1)]),
  }
  computed integer: Option<Int> = Int::from(boundary);
  computed refused_integer: Option<Int> = Int::from(fractional_boundary);
  computed mapped: Option<Seq<Int>> = increment(values);
  computed refused_map: Option<Seq<Int>> = increment(invalid_values);
  computed mapped_values: Option<Map<Text, Int>> = increment_values(map);
  computed refused_mapped_values: Option<Map<Text, Int>> =
    increment_values(invalid_map);
  computed unique_map: Option<Map<Text, Int>> = Map::from_unique(pairs);
  computed refused_unique_map: Option<Map<Text, Int>> =
    Map::from_unique(duplicate_pairs);
  computed refused_unique_set: Option<Set<Int>> =
    Set::from_unique(duplicates);
  computed unique_set: Option<Set<Int>> = Set::from_unique(unique_values);
  computed ordered: Seq<(Text, Int)> = map.entries_by_key();
  observe {
    integer,
    refused_integer,
    mapped,
    refused_map,
    mapped_values,
    refused_mapped_values,
    unique_map,
    refused_unique_map,
    refused_unique_set,
    unique_set,
    ordered,
  }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "A0 helper diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked A0 helper inventory");
    let (instance, _) = program
        .admit(
            "example.standard@1::A0Helpers",
            Value::Unit,
            "standard/a0-helpers",
        )
        .expect("A0 helper admission");

    assert_eq!(
        constructor(record_field(&instance.observation, "integer")),
        "some"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "refused_integer")),
        "none"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "mapped")),
        "some"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "refused_map")),
        "none"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "mapped_values")),
        "some"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "refused_mapped_values")),
        "none"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "unique_map")),
        "some"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "refused_unique_map")),
        "none"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "refused_unique_set")),
        "none"
    );
    assert_eq!(
        constructor(record_field(&instance.observation, "unique_set")),
        "some"
    );
    assert_eq!(
        record_field(&instance.observation, "ordered"),
        &Value::Seq(vec![
            Value::Tuple(vec![Value::Text("a".into()), Value::int(1)]),
            Value::Tuple(vec![Value::Text("b".into()), Value::int(2)]),
        ])
    );
}

#[test]
fn ratio_multiplication_and_proved_addition_or_subtraction_are_total() {
    let output = check(
        r#"
fn product(left: Ratio, right: Ratio) -> Ratio {
  left * right
}

fn difference(left: Ratio, right: Ratio) -> Ratio {
  if right <= left { left - right } else { left }
}

fn add_from_zero(left: Ratio, right: Ratio) -> Ratio {
  if left == 0.0 { left + right } else { right }
}

pub machine RatioProbe {
  state {
    half: Ratio = 0.5,
    quarter: Ratio = 0.25,
    zero: Ratio = 0.0,
  }
  observe {
    product: product(half, quarter),
    difference: difference(half, quarter),
    sum: add_from_zero(zero, half),
  }
}
"#,
    );
    assert!(
        output.diagnostics.is_empty(),
        "Ratio diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked Ratio proof");
    let (instance, _) = program
        .admit(
            "example.standard@1::RatioProbe",
            Value::Unit,
            "standard/ratio",
        )
        .expect("Ratio probe admission");
    assert_eq!(
        record_field(&instance.observation, "product"),
        &Value::Ratio(Decimal::from_str("0.125").expect("decimal"))
    );
    assert_eq!(
        record_field(&instance.observation, "difference"),
        &Value::Ratio(Decimal::from_str("0.25").expect("decimal"))
    );
    assert_eq!(
        record_field(&instance.observation, "sum"),
        &Value::Ratio(Decimal::from_str("0.5").expect("decimal"))
    );

    for source in [
        "fn invalid(left: Ratio, right: Ratio) -> Ratio { left + right }",
        "fn invalid(left: Ratio, right: Ratio) -> Ratio { left - right }",
    ] {
        let invalid = check(source);
        assert!(invalid.program.is_none());
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "uhura/ratio-arithmetic"),
            "missing Ratio proof diagnostic: {:?}",
            invalid.diagnostics
        );
    }
}

fn constructor(value: &Value) -> &str {
    let Value::Variant { constructor, .. } = value else {
        panic!("expected variant, found {value:?}");
    };
    constructor
}

#[test]
fn global_constant_cycles_are_rejected_deterministically() {
    let source = r#"
pub const B: Int = A;
pub const A: Int = B;

pub machine CycleProbe {
  outcomes { commit Done }
}
"#;
    let first = check(source);
    let second = check(source);
    assert!(first.program.is_none());
    let cycles = |output: &uhura_check::CheckOutput| {
        output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "uhura/recursive-constant")
            .map(|diagnostic| {
                (
                    diagnostic.span.start,
                    diagnostic.message.clone(),
                    diagnostic.code.to_string(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(cycles(&first), cycles(&second));
    assert_eq!(cycles(&first).len(), 2);
    assert!(
        first
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule != "uhura/non-constant-expression")
    );
}
