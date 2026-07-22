//! Deterministic scaling contract for the browser event hot path.
//!
//! Wall-clock assertions are deliberately absent. This fixture grows a real
//! admitted session's receipt history while holding the machine and projected
//! UI shape constant, then compares the transported JSON structure at two
//! equal-width sequence numbers.

use std::collections::BTreeSet;

use serde_json::{Value as JsonValue, json};
use uhura_base::to_canonical_json;
use uhura_check::check_project_modules;
use uhura_syntax::{Module, SourceIdentity, parse};
use uhura_wasm::{BROWSER_PROTOCOL, Session};

const PACKAGE: &str = "example.dispatch-scaling@1";
const MACHINE_ID: &str = "example.dispatch-scaling@1::Counter";
const PRESENTATION_ID: &str = "example.dispatch-scaling@1::CounterWeb";

const MACHINE: &str = r#"
pub machine Counter {
  events {
    Increment,
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = 0,
  }

  observe {
    count,
  }

  on Increment {
    count = count + 1;
    Accepted
  }
}
"#;

const UI: &str = r#"
use uhura::ui;
use crate::counter::Counter;

pub ui CounterWeb for Counter(view) {
  <button on press -> Increment>{view.count}</button>
}
"#;

fn module(file: u32, logical: &str, source: &str) -> Module {
    let parsed = parse(
        SourceIdentity::new(file, PACKAGE, logical, format!("{logical}.uhura")),
        source,
    );
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics for {logical}:\n{:#?}",
        parsed.diagnostics
    );
    parsed.module
}

fn session() -> Session {
    let checked = check_project_modules(&[module(1, "counter", MACHINE), module(2, "web", UI)]);
    assert!(
        checked.diagnostics.is_empty(),
        "checker diagnostics:\n{:#?}",
        checked.diagnostics
    );
    let program = checked.program.expect("counter fixture checks");
    let expected_identity = to_canonical_json(&json!({
        "identityProtocol": program.machine_program.identity_protocol,
        "machineProgramHash": program.machine_program.program_hashes[MACHINE_ID],
        "presentationHash": program.presentation_hashes[PRESENTATION_ID],
    }));
    Session::new(
        &program.to_canonical_string(),
        MACHINE_ID,
        &to_canonical_json(&json!({ "$": "unit" })),
        "performance/dispatch-scaling",
        Some(PRESENTATION_ID.into()),
        &expected_identity,
    )
    .expect("counter fixture admits")
}

fn empty_ui_event() -> String {
    to_canonical_json(&json!({
        "$": "record",
        "fields": [],
    }))
}

fn json(source: &str) -> JsonValue {
    serde_json::from_str(source).expect("runtime transport is JSON")
}

/// Counts transported containers and values without depending on field text or
/// allocator behavior. A retained receipt added to a step necessarily raises
/// this measure; changing a same-width sequence or state value does not.
fn transport_footprint(value: &JsonValue) -> usize {
    match value {
        JsonValue::Array(values) => 1 + values.iter().map(transport_footprint).sum::<usize>(),
        JsonValue::Object(fields) => {
            1 + fields.len() + fields.values().map(transport_footprint).sum::<usize>()
        }
        _ => 1,
    }
}

fn snapshot_keys(step: &JsonValue) -> BTreeSet<&str> {
    step["snapshot"]
        .as_object()
        .expect("browser step has a bounded runtime snapshot")
        .keys()
        .map(String::as_str)
        .collect()
}

fn object_keys(value: &JsonValue) -> BTreeSet<&str> {
    value
        .as_object()
        .expect("transport value is an object")
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn browser_step_transport_is_independent_of_retained_receipt_depth() {
    const EARLY: usize = 20;
    const LATE: usize = 80;

    let mut session = session();
    let event = empty_ui_event();
    let initial_presentation = json(&session.presentation());
    let mut view = initial_presentation["view"].clone();
    let mut projection_revision = initial_presentation["projectionRevision"]
        .as_str()
        .expect("initial counter projection revision is exact text")
        .to_string();
    let mut early_step = None;
    let mut early_inspection = None;
    let mut late_step = None;
    let mut late_inspection = None;

    for sequence in 1..=LATE {
        let binding = view["nodes"][0]["events"][0]["binding"]
            .as_str()
            .expect("counter button has one event binding");
        let step = session
            .dispatch_ui(binding, &projection_revision, &event)
            .unwrap_or_else(|error| panic!("reaction {sequence}: {error}"));
        let decoded_step = json(&step);
        view = decoded_step["presentation"]["view"].clone();
        projection_revision = decoded_step["presentation"]["projectionRevision"]
            .as_str()
            .expect("counter projection revision is exact text")
            .to_string();
        match sequence {
            EARLY => {
                early_step = Some(step);
                early_inspection = Some(session.inspect().expect("early full inspection"));
            }
            LATE => {
                late_step = Some(step);
                late_inspection = Some(session.inspect().expect("late full inspection"));
            }
            _ => {}
        }
    }

    let early_source = early_step.expect("early step captured");
    let late_source = late_step.expect("late step captured");
    let early = json(&early_source);
    let late = json(&late_source);

    assert_eq!(early["protocol"], BROWSER_PROTOCOL);
    assert_eq!(late["protocol"], BROWSER_PROTOCOL);
    assert_eq!(
        object_keys(&early),
        BTreeSet::from(["presentation", "protocol", "receipt", "snapshot"]),
        "the reaction hot path must transport only its receipt, bounded current snapshot, and presentation"
    );
    assert_eq!(object_keys(&late), object_keys(&early));
    assert_eq!(
        snapshot_keys(&early),
        BTreeSet::from([
            "configurationHash",
            "ingressPrefixHash",
            "instance",
            "lifecycle",
            "machineProgramHash",
            "nextIngressOrdinal",
            "nextSequence",
            "presentation",
            "presentationHash",
            "protocol",
            "state",
            "stateHash",
            "tracePrefixHash",
        ]),
        "the hot-path snapshot must remain current-state-only"
    );
    assert_eq!(snapshot_keys(&late), snapshot_keys(&early));
    assert_eq!(
        transport_footprint(&late),
        transport_footprint(&early),
        "browser-step structure grew with retained receipt depth"
    );
    assert!(
        late_source.len() <= early_source.len() + 8,
        "same-width browser step grew from {} to {} bytes",
        early_source.len(),
        late_source.len()
    );

    let early_inspection = json(&early_inspection.expect("early inspection captured"));
    let late_inspection = json(&late_inspection.expect("late inspection captured"));
    assert_eq!(
        early_inspection["receipts"]
            .as_array()
            .expect("full inspection retains receipts")
            .len(),
        EARLY + 1,
    );
    assert_eq!(
        late_inspection["receipts"]
            .as_array()
            .expect("full inspection retains receipts")
            .len(),
        LATE + 1,
    );
    assert!(
        transport_footprint(&late_inspection) > transport_footprint(&early_inspection) * 2,
        "fixture did not grow the explicit diagnostic history"
    );
}
