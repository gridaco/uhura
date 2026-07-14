//! The frozen ABI contract (design §12.3, plan micro-decision #14).
//!
//! Two layers:
//! 1. **Shape goldens** — the hand-written JSON forms of events, the
//!    step-result envelope, and the protocol triple, pinned as literal
//!    strings. `web/src/protocol/types.ts` mirrors these types; a change here is
//!    a protocol version bump, not an edit.
//! 2. **Pump parity** — every canonical corpus script replayed through
//!    `Session` + `FixtureDriver` (natively — the crate is an rlib too)
//!    against the native trace harness, asserting BYTE-equal trace lines.
//!    This is the native half of the M6 wasm parity gate: the wasm build
//!    wraps exactly this code, so what's proven here is the semantics;
//!    `scripts/parity.mjs` proves the compiled artifact.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uhura_base::{Ident, Severity, Value, to_canonical_json};
use uhura_check::fixture::load_fixture;
use uhura_cli::cmd::trace::{boot_updates, fixture_slices_json, run_script};
use uhura_core::event::Event;
use uhura_core::view::{Descriptor, DescriptorKind};
use uhura_port::envelope::{OutcomeResult, ProjectionUpdate};
use uhura_wasm::{FixtureDriverJs, Session, protocols};

fn ident(s: &str) -> Ident {
    Ident::new(s).unwrap()
}

// ── layer 1: shape goldens ──────────────────────────────────────────────────

#[test]
fn protocol_triple_is_pinned() {
    assert_eq!(
        protocols(),
        r#"{"ir":"uhura-ir/0","provider":"uhura-provider/0","view":"uhura-view/0"}"#
    );
}

#[test]
fn event_wire_shapes_are_pinned_and_round_trip() {
    let descriptor = Descriptor {
        kind: DescriptorKind::Input,
        event: ident("press"),
        emit: ident("like-toggled"),
        scope: "page:1".into(),
        payload: serde_json::json!({ "post": "post-lena-glaze" }),
        carries: BTreeMap::new(),
    };
    let ui = Event::Ui {
        descriptor,
        data: None,
        view_rev: 4,
    };
    assert_eq!(
        to_canonical_json(&ui.to_json()),
        r#"{"descriptor":{"emit":"like-toggled","event":"press","kind":"input","payload":{"post":"post-lena-glaze"},"scope":"page:1"},"kind":"ui","view-rev":4}"#
    );

    let mut carries = BTreeMap::new();
    carries.insert(ident("value"), "text".to_string());
    let mut data = BTreeMap::new();
    data.insert(ident("value"), Value::Text("Saving this recipe".into()));
    let ui_carried = Event::Ui {
        descriptor: Descriptor {
            kind: DescriptorKind::Input,
            event: ident("change"),
            emit: ident("draft-changed"),
            scope: "surface:1".into(),
            payload: serde_json::json!({}),
            carries,
        },
        data: Some(Value::Record(data)),
        view_rev: 9,
    };
    assert_eq!(
        to_canonical_json(&ui_carried.to_json()),
        r#"{"data":{"value":"Saving this recipe"},"descriptor":{"carries":{"value":"text"},"emit":"draft-changed","event":"change","kind":"input","payload":{},"scope":"surface:1"},"kind":"ui","view-rev":9}"#
    );

    let outcome = Event::Outcome {
        correlation: "c-1".into(),
        result: OutcomeResult::Refused {
            refusal: ident("rate-limited"),
        },
        updates: vec![ProjectionUpdate {
            port: ident("feed"),
            projection: ident("feed-page"),
            key: None,
            revision: 3,
            value: serde_json::json!({ "has-more": false }),
        }],
    };
    assert_eq!(
        to_canonical_json(&outcome.to_json()),
        r#"{"correlation":"c-1","kind":"outcome","outcome":{"refused":{"refusal":"rate-limited"}},"updates":[{"key":null,"port":"feed","projection":"feed-page","revision":3,"value":{"has-more":false}}]}"#
    );

    let failed = Event::ProjectionFailed {
        port: ident("feed"),
        projection: ident("feed-page"),
        key: None,
        reason: "unreachable".into(),
    };
    assert_eq!(
        to_canonical_json(&failed.to_json()),
        r#"{"key":null,"kind":"projection-failed","port":"feed","projection":"feed-page","reason":"unreachable"}"#
    );

    let mut params = BTreeMap::new();
    params.insert(ident("user"), Value::Id("user-lena".into()));
    let init = Event::Init {
        route: ident("profile"),
        params,
    };
    assert_eq!(
        to_canonical_json(&init.to_json()),
        r#"{"kind":"init","params":{"user":"user-lena"},"route":"profile"}"#
    );

    // Every variant round-trips through the wire form.
    for event in [
        ui,
        ui_carried,
        outcome,
        Event::Projection {
            updates: vec![ProjectionUpdate {
                port: ident("comments"),
                projection: ident("for-post"),
                key: Some(serde_json::json!("post-lena-glaze")),
                revision: 2,
                value: serde_json::json!({ "comments": [] }),
            }],
        },
        failed,
        init,
    ] {
        let json = event.to_json();
        assert_eq!(Event::from_json(&json).unwrap(), event, "{json}");
    }
}

#[test]
fn wire_init_params_decode_strings_as_ids() {
    // Micro-decision #56: route params are entity references (§3).
    let event = Event::from_json(&serde_json::json!({
        "kind": "init",
        "route": "profile",
        "params": { "user": "user-lena" },
    }))
    .unwrap();
    let Event::Init { params, .. } = event else {
        panic!("parsed a non-init event");
    };
    assert_eq!(params[&ident("user")], Value::Id("user-lena".into()));
}

#[test]
fn carried_data_is_typed_by_the_descriptor() {
    // A field the descriptor does not declare is refused — the renderer
    // trust boundary is exactly the declared carries (§4.2).
    let err = Event::from_json(&serde_json::json!({
        "kind": "ui",
        "descriptor": {
            "kind": "input", "event": "press", "emit": "like-toggled",
            "scope": "page:1", "payload": {},
        },
        "data": { "value": "sneaky" },
        "view-rev": 1,
    }))
    .unwrap_err();
    assert!(err.contains("does not carry"), "{err}");
}

// ── layer 2: pump parity against the native trace harness ──────────────────

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/instagram-uhura")
}

fn checked_program() -> uhura_core::ir::ProgramIr {
    let input = uhura_cli::cmd::assemble_input(&corpus_root()).expect("corpus reads");
    let output = uhura_check::check(&input);
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "the corpus must check clean"
    );
    output.lowered.expect("lowered program").program
}

struct Stimulus {
    at_tick: u64,
    emit: String,
    where_: serde_json::Map<String, serde_json::Value>,
    data: serde_json::Map<String, serde_json::Value>,
}

fn parse_stimuli(script: &serde_json::Value) -> Vec<Stimulus> {
    let Some(entries) = script.get("ui").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|entry| Stimulus {
            at_tick: entry
                .get("at-tick")
                .and_then(serde_json::Value::as_u64)
                .unwrap(),
            emit: entry
                .get("emit")
                .and_then(serde_json::Value::as_str)
                .unwrap()
                .to_string(),
            where_: match entry.get("where") {
                Some(serde_json::Value::Object(m)) => m.clone(),
                _ => serde_json::Map::new(),
            },
            data: match entry.get("data") {
                Some(serde_json::Value::Object(m)) => m.clone(),
                _ => serde_json::Map::new(),
            },
        })
        .collect()
}

fn descriptor_matches(
    d: &serde_json::Value,
    emit: &str,
    where_: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    d.get("emit").and_then(serde_json::Value::as_str) == Some(emit)
        && where_
            .iter()
            .all(|(k, v)| d.get("payload").and_then(|p| p.get(k)) == Some(v))
}

fn collect_matches<'a>(
    node: &'a serde_json::Value,
    emit: &str,
    where_: &serde_json::Map<String, serde_json::Value>,
    out: &mut Vec<&'a serde_json::Value>,
) {
    if let Some(on) = node.get("on").and_then(serde_json::Value::as_array) {
        for d in on {
            if descriptor_matches(d, emit, where_) {
                out.push(d);
            }
        }
    }
    if let Some(children) = node.get("children").and_then(serde_json::Value::as_array) {
        for child in children {
            collect_matches(child, emit, where_, out);
        }
    }
}

/// The JSON-only pump — exactly what the play shell does, minus the DOM:
/// find the stimulus descriptor in the CURRENT view JSON, echo it into a
/// `ui` event, forward emitted commands to the driver.
fn find_descriptor(view: &serde_json::Value, stim: &Stimulus) -> serde_json::Value {
    let mut matches = Vec::new();
    collect_matches(
        &view["page"]["root"],
        &stim.emit,
        &stim.where_,
        &mut matches,
    );
    for surface in view["surfaces"].as_array().unwrap() {
        collect_matches(&surface["root"], &stim.emit, &stim.where_, &mut matches);
        if descriptor_matches(&surface["dismiss"], &stim.emit, &stim.where_) {
            matches.push(&surface["dismiss"]);
        }
    }
    let mut distinct: Vec<&serde_json::Value> = Vec::new();
    for d in matches {
        if !distinct.iter().any(|seen| {
            seen["emit"] == d["emit"]
                && seen["scope"] == d["scope"]
                && seen["payload"] == d["payload"]
        }) {
            distinct.push(d);
        }
    }
    assert_eq!(
        distinct.len(),
        1,
        "stimulus `{}` must match exactly one distinct descriptor",
        stim.emit
    );
    distinct[0].clone()
}

/// Replays one script through the wasm-facing Session/Driver pair and
/// returns the canonical trace lines (`t` of each step-result).
fn run_session(program: &uhura_core::ir::ProgramIr, script_name: &str) -> Vec<String> {
    let root = corpus_root();
    let fixture_text = std::fs::read_to_string(root.join("fixtures/standard.toml")).unwrap();
    let script_text =
        std::fs::read_to_string(root.join(format!("fixtures/scripts/{script_name}.toml"))).unwrap();
    let fixture = load_fixture(&fixture_text).expect("fixture loads");
    let script_json = uhura_fixture::toml_to_json(&script_text).expect("script converts");
    let stimuli = parse_stimuli(&script_json);

    let mut session = Session::new(&program.to_canonical_string()).expect("IR loads");
    let boot = boot_updates(program, &fixture).expect("boot slices");
    let boot_json = serde_json::json!({
        "updates": boot.iter().map(ProjectionUpdate::to_json).collect::<Vec<_>>(),
    });
    session
        .boot(&to_canonical_json(&boot_json))
        .expect("boot applies");
    let mut driver = FixtureDriverJs::new(
        &fixture_slices_json(&fixture),
        &to_canonical_json(&script_json),
    )
    .expect("driver builds");

    let mut lines = Vec::new();
    let mut dispatch = |session: &mut Session,
                        driver: &mut FixtureDriverJs,
                        event: serde_json::Value|
     -> serde_json::Value {
        let out = session
            .dispatch(&to_canonical_json(&event))
            .expect("dispatch succeeds");
        let result: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            result.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["c", "g", "i", "t", "v"],
            "the step-result envelope is frozen"
        );
        lines.push(to_canonical_json(&result["t"]));
        for c in result["c"].as_array().unwrap() {
            driver.deliver(&to_canonical_json(c)).expect("deliver");
        }
        result
    };

    let mut view = dispatch(
        &mut session,
        &mut driver,
        serde_json::json!({ "kind": "init", "route": program.entry.to_string(), "params": {} }),
    )["v"]
        .clone();

    let mut tick = 0u64;
    let mut next = 0usize;
    loop {
        if driver.idle() && next >= stimuli.len() {
            break;
        }
        tick += 1;
        assert!(tick <= 10_000, "the script must quiesce");

        for msg_json in driver.tick() {
            let msg: serde_json::Value = serde_json::from_str(&msg_json).unwrap();
            // Provider messages map to events: a standalone projection
            // update wraps into an `updates` list; outcome and
            // projection-failed pass through shape-identical.
            let event = match msg["kind"].as_str().unwrap() {
                "projection" => serde_json::json!({ "kind": "projection", "updates": [msg] }),
                "outcome" | "projection-failed" => msg,
                other => panic!("the driver emitted `{other}`"),
            };
            view = dispatch(&mut session, &mut driver, event)["v"].clone();
        }

        while next < stimuli.len() && stimuli[next].at_tick == tick {
            let stim = &stimuli[next];
            next += 1;
            let descriptor = find_descriptor(&view, stim);
            let mut event = serde_json::json!({
                "kind": "ui",
                "descriptor": descriptor,
                "view-rev": view["revision"],
            });
            if !stim.data.is_empty() {
                event["data"] = serde_json::Value::Object(stim.data.clone());
            }
            view = dispatch(&mut session, &mut driver, event)["v"].clone();
        }
    }

    assert_eq!(
        session.view().expect("a view exists"),
        to_canonical_json(&view),
        "`view()` returns the last step's snapshot"
    );
    lines
}

#[test]
fn session_pump_matches_the_native_harness_byte_for_byte() {
    let program = checked_program();
    let root = corpus_root();
    let fixture_text = std::fs::read_to_string(root.join("fixtures/standard.toml")).unwrap();
    for script in [
        "like-ok",
        "like-refused",
        "comment-ok",
        "paginate",
        "feed-failed",
        "feed-empty",
        "demo",
    ] {
        let script_text =
            std::fs::read_to_string(root.join(format!("fixtures/scripts/{script}.toml"))).unwrap();
        let native = run_script(&program, &fixture_text, &script_text, false).expect("native run");
        let session = run_session(&program, script);
        assert_eq!(session, native, "trace lines diverged for `{script}`");
    }
}

#[test]
fn session_refuses_foreign_ir_and_premature_views() {
    assert!(Session::new(r#"{"protocol":"uhura-ir/1"}"#).is_err());
    let program = checked_program();
    let session = Session::new(&program.to_canonical_string()).unwrap();
    assert_eq!(session.ir_version(), "uhura-ir/0");
    assert_eq!(session.revision(), 0.0);
    assert!(session.view().is_err(), "no view before the first dispatch");
}
