//! The transactional-dispatch test matrix (plan risk #3) — written from
//! design §4.2/§7.2 BEFORE the interpreter, against a synthetic program.
//!
//! Axes covered:
//! - abort point × statement: guard not-ready (guard is false), body
//!   not-ready after a write / after a send (atomic abort: no writes, no
//!   pending, no commands, counters rolled back), clean commit per
//!   statement kind (set field / set map / send-with-bind / open-surface /
//!   dismiss / navigate / back);
//! - scope: the same txn rules from page and surface origin;
//! - acceptance (§7.2, in order): stale-scope, occluded, ineligible —
//!   with stale `view_rev` ACCEPTED;
//! - multi-handler guard selection: source order, first satisfied wins,
//!   none ⇒ dropped `no-handler`;
//! - outcome routing: `.ok`/`.err` by origin, `refusal` binding
//!   (`unavailable` routes to `.err`), unknown correlation, unmounted
//!   origin ⇒ `stale-outcome` with pending removed;
//! - structure: idempotent open per (definition, canonical context),
//!   dismiss + FocusRestore when topmost, navigate/back with History
//!   intents, force-close cascade, nav underflow;
//! - `rev + 1` always — commits, aborts, and every drop.

use std::collections::BTreeMap;

use uhura_base::{Ident, Value};
use uhura_core::event::Event;
use uhura_core::ir::{self, ProgramIr, TyIr};
use uhura_core::state::{Projections, UiState};
use uhura_core::step::{StepResult, step_u};
use uhura_core::trace::{Disposition, DropReason, GuardResult};
use uhura_core::view::{Descriptor, DescriptorKind};
use uhura_port::envelope::OutcomeResult;

fn ident(s: &str) -> Ident {
    Ident::new(s).unwrap()
}

// ── the synthetic program ───────────────────────────────────────────────────

fn set(field: &str, key: Option<ir::ExprIr>, value: ir::ExprIr) -> ir::StmtIr {
    ir::StmtIr::Set {
        field: ident(field),
        key,
        value,
    }
}

fn send_ping(k: ir::ExprIr, bind: Option<&str>) -> ir::StmtIr {
    ir::StmtIr::Send {
        port: ident("svc"),
        command: ident("ping"),
        args: vec![ir::ArgIr {
            name: ident("k"),
            value: k,
        }],
        bind: bind.map(ident),
    }
}

fn handler(
    event: &str,
    params: &[&str],
    guard: Option<ir::ExprIr>,
    body: Vec<ir::StmtIr>,
) -> ir::HandlerIr {
    ir::HandlerIr {
        on: ir::EventKeyIr::Semantic {
            event: ident(event),
        },
        params: params.iter().map(|p| ident(p)).collect(),
        guard,
        body,
    }
}

fn outcome_handler(command: &str, which: ir::OutcomeKindIr, params: &[&str]) -> ir::HandlerIr {
    ir::HandlerIr {
        on: ir::EventKeyIr::Outcome {
            command: ident(command),
            which,
        },
        params: params.iter().map(|p| ident(p)).collect(),
        guard: None,
        body: vec![],
    }
}

fn view_root(children: Vec<ir::NodeIr>) -> ir::NodeIr {
    ir::NodeIr::Element(ir::ElementIr {
        element: ident("view"),
        ord: 0,
        class: None,
        props: vec![],
        events: vec![],
        text: vec![],
        children,
    })
}

fn program() -> ProgramIr {
    use ir::ExprIr as E;

    let binding = |name: &str| E::BindingRef(ident(name));
    let state_ref = |name: &str| E::StateRef(ident(name));
    let eq = |lhs: E, rhs: E| E::Binary {
        op: ir::BinaryOpIr::Eq,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    };
    let add = |lhs: E, rhs: E| E::Binary {
        op: ir::BinaryOpIr::Add,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    };

    let mut home_events = BTreeMap::new();
    for (event, params) in [
        ("bump", vec![]),
        ("seq", vec![]),
        ("bump-if-ready", vec![]),
        ("write-then-stall", vec![]),
        ("send-ping", vec![(ident("x"), TyIr::Int)]),
        ("open-panel", vec![(ident("p"), TyIr::Id)]),
        ("flip-open", vec![(ident("c"), TyIr::Bool)]),
        ("nav-other", vec![]),
        ("nav-back", vec![]),
        ("multi", vec![(ident("x"), TyIr::Int)]),
    ] {
        home_events.insert(
            ident(event),
            params
                .into_iter()
                .map(|(name, ty)| ir::EventParamIr { name, ty })
                .collect(),
        );
    }

    let home = ir::DefIr {
        modality: None,
        props: vec![],
        emits: vec![],
        params: vec![],
        state: BTreeMap::from([
            (ident("n"), ir::InitValue::Int(0)),
            (ident("m"), ir::InitValue::EmptyMap),
            (ident("note"), ir::InitValue::None),
            (ident("flag"), ir::InitValue::Bool(false)),
        ]),
        events: home_events,
        handlers: vec![
            // 0 — set field commit
            handler(
                "bump",
                &[],
                None,
                vec![set("n", None, add(state_ref("n"), E::Int(1)))],
            ),
            // 1 — writes visible sequentially within the handler
            handler(
                "seq",
                &[],
                None,
                vec![
                    set("n", None, E::Int(1)),
                    set("n", None, add(state_ref("n"), state_ref("n"))),
                ],
            ),
            // 2 — guard-position not-ready ⇒ guard is false
            handler(
                "bump-if-ready",
                &[],
                Some(eq(
                    E::ProjectionRef(ident("spare-proj")),
                    E::Text("go".into()),
                )),
                vec![set("n", None, E::Int(42))],
            ),
            // 3 — body not-ready after a write and a send ⇒ atomic abort
            handler(
                "write-then-stall",
                &[],
                None,
                vec![
                    set("n", None, E::Int(5)),
                    send_ping(E::Int(9), None),
                    set("note", None, E::ProjectionRef(ident("spare-proj"))),
                ],
            ),
            // 4 — send with tag bind + tag-keyed map write
            handler(
                "send-ping",
                &["x"],
                None,
                vec![
                    send_ping(binding("x"), Some("t")),
                    set("m", Some(binding("t")), E::Bool(true)),
                ],
            ),
            // 5 — open-surface with canonical context from the payload
            handler(
                "open-panel",
                &["p"],
                None,
                vec![ir::StmtIr::OpenSurface {
                    surface: ident("panel"),
                    args: vec![ir::ArgIr {
                        name: ident("who"),
                        value: binding("p"),
                    }],
                }],
            ),
            // Writes state its own trigger's payload reads, THEN opens —
            // the focus search must see the pre-commit view (§4.2).
            handler(
                "flip-open",
                &["c"],
                None,
                vec![
                    set(
                        "flag",
                        None,
                        E::Unary {
                            op: ir::UnaryOpIr::Not,
                            expr: Box::new(binding("c")),
                        },
                    ),
                    ir::StmtIr::OpenSurface {
                        surface: ident("panel"),
                        args: vec![ir::ArgIr {
                            name: ident("who"),
                            value: E::Text("f".into()),
                        }],
                    },
                ],
            ),
            // 6 / 7 — navigation
            handler(
                "nav-other",
                &[],
                None,
                vec![ir::StmtIr::Navigate {
                    route: ident("other"),
                    args: vec![],
                }],
            ),
            handler("nav-back", &[], None, vec![ir::StmtIr::NavigateBack]),
            // 8 / 9 — multi-handler: source order, first satisfied guard
            handler(
                "multi",
                &["x"],
                Some(eq(binding("x"), E::Int(1))),
                vec![set("n", None, E::Int(1))],
            ),
            handler(
                "multi",
                &["x"],
                Some(eq(binding("x"), E::Int(2))),
                vec![set("n", None, E::Int(2))],
            ),
            // 10 / 11 — outcome handlers
            {
                let mut h = outcome_handler("ping", ir::OutcomeKindIr::Ok, &["tag", "cmd"]);
                h.body = vec![
                    set("m", Some(binding("tag")), E::None),
                    set(
                        "n",
                        None,
                        E::Field {
                            base: Box::new(binding("cmd")),
                            name: ident("k"),
                        },
                    ),
                ];
                h
            },
            {
                let mut h =
                    outcome_handler("ping", ir::OutcomeKindIr::Err, &["tag", "cmd", "refusal"]);
                h.body = vec![set("note", None, binding("refusal"))];
                h
            },
        ],
        root: view_root(vec![
            ir::NodeIr::Element(ir::ElementIr {
                element: ident("button"),
                ord: 1,
                class: None,
                props: vec![],
                events: vec![ir::ElementEventBindingIr {
                    event: ident("press"),
                    emit: ident("open-panel"),
                    args: vec![ir::ArgIr {
                        name: ident("p"),
                        value: E::Text("a".into()),
                    }],
                }],
                text: vec![],
                children: vec![],
            }),
            ir::NodeIr::Element(ir::ElementIr {
                element: ident("button"),
                ord: 2,
                class: None,
                props: vec![],
                events: vec![ir::ElementEventBindingIr {
                    event: ident("press"),
                    emit: ident("flip-open"),
                    args: vec![ir::ArgIr {
                        name: ident("c"),
                        value: E::StateRef(ident("flag")),
                    }],
                }],
                text: vec![],
                children: vec![],
            }),
        ]),
    };

    let panel = ir::DefIr {
        modality: Some("sheet".into()),
        props: vec![ident("who")],
        emits: vec![],
        params: vec![],
        state: BTreeMap::from([(ident("c"), ir::InitValue::Int(0))]),
        events: BTreeMap::from([
            (ident("close"), vec![]),
            (ident("panel-bump"), vec![]),
            (ident("panel-send"), vec![]),
            (ident("panel-stall"), vec![]),
            (ident("panel-back"), vec![]),
            (ident("reopen"), vec![]),
            (ident("respawn"), vec![]),
        ]),
        handlers: vec![
            handler("close", &[], None, vec![ir::StmtIr::Dismiss]),
            handler("panel-back", &[], None, vec![ir::StmtIr::NavigateBack]),
            // Opens its own (definition, context) — the idempotence cell.
            handler(
                "reopen",
                &[],
                None,
                vec![ir::StmtIr::OpenSurface {
                    surface: ident("panel"),
                    args: vec![ir::ArgIr {
                        name: ident("who"),
                        value: ir::ExprIr::PropRef(ident("who")),
                    }],
                }],
            ),
            // Dismisses itself, then opens — the opener dies mid-dispatch.
            handler(
                "respawn",
                &[],
                None,
                vec![
                    ir::StmtIr::Dismiss,
                    ir::StmtIr::OpenSurface {
                        surface: ident("panel"),
                        args: vec![ir::ArgIr {
                            name: ident("who"),
                            value: ir::ExprIr::Text("r".into()),
                        }],
                    },
                ],
            ),
            handler(
                "panel-bump",
                &[],
                None,
                vec![set("c", None, add(state_ref("c"), E::Int(1)))],
            ),
            handler("panel-send", &[], None, vec![send_ping(E::Int(1), None)]),
            handler(
                "panel-stall",
                &[],
                None,
                vec![
                    set("c", None, E::Int(99)),
                    set("c", None, E::ProjectionRef(ident("spare-proj"))),
                ],
            ),
        ],
        root: view_root(vec![]),
    };

    let other = ir::DefIr {
        modality: None,
        props: vec![],
        emits: vec![],
        params: vec![],
        state: BTreeMap::new(),
        events: BTreeMap::new(),
        handlers: vec![],
        root: view_root(vec![]),
    };

    let projection = |port: &str| ir::ProjectionIr {
        port: ident(port),
        boot: false,
        ty: TyIr::Text,
        key: None,
    };

    ProgramIr {
        protocol: ir::IR_PROTOCOL.to_string(),
        app: ident("matrix"),
        entry: ident("home"),
        catalog: ir::CatalogPin {
            name: ident("base"),
            version: "0.1.0".into(),
            hash: "0".repeat(64),
        },
        ports: BTreeMap::from([(
            ident("svc"),
            ir::PortPin {
                version: "0.1.0".into(),
                hash: "0".repeat(64),
            },
        )]),
        projections: BTreeMap::from([
            (ident("ready-proj"), projection("svc")),
            (ident("spare-proj"), projection("svc")),
        ]),
        element_events: BTreeMap::from([(
            ident("button"),
            BTreeMap::from([(
                ident("press"),
                ir::ElementEventIr {
                    kind: ir::EventKindIr::Input,
                    carries: BTreeMap::new(),
                },
            )]),
        )]),
        element_props: BTreeMap::new(),
        routes: BTreeMap::from([
            (
                ident("home"),
                ir::RouteIr {
                    segments: vec![ir::RouteSegIr::Static("home".into())],
                    params: vec![],
                },
            ),
            (
                ident("other"),
                ir::RouteIr {
                    segments: vec![ir::RouteSegIr::Static("other".into())],
                    params: vec![],
                },
            ),
        ]),
        pages: BTreeMap::from([(ident("home"), home), (ident("other"), other)]),
        components: BTreeMap::new(),
        surfaces: BTreeMap::from([(ident("panel"), panel)]),
    }
}

// ── harness helpers ─────────────────────────────────────────────────────────

fn projections() -> Projections {
    let mut x = Projections::default();
    x.snapshots.insert(
        (ident("ready-proj"), None),
        uhura_core::state::ProjectionSnapshot {
            revision: 1,
            value: Value::Text("go".into()),
        },
    );
    x
}

fn ui(emit: &str, scope: &str, payload: serde_json::Value, view_rev: u64) -> Event {
    Event::Ui {
        descriptor: Descriptor {
            kind: DescriptorKind::Input,
            event: ident("press"),
            emit: ident(emit),
            scope: scope.to_string(),
            payload,
            carries: BTreeMap::new(),
        },
        data: None,
        view_rev,
    }
}

fn booted(p: &ProgramIr) -> UiState {
    let x = projections();
    step_u(
        p,
        UiState::boot(),
        &x,
        Event::Init {
            route: ident("home"),
            params: BTreeMap::new(),
        },
    )
    .expect("init")
    .u
}

fn run(p: &ProgramIr, u: UiState, e: Event) -> StepResult {
    let x = projections();
    step_u(p, u, &x, e).expect("step")
}

fn page_state<'a>(u: &'a UiState, field: &str) -> &'a Value {
    &u.nav.last().unwrap().state[&ident(field)]
}

fn drop_reason(r: &StepResult) -> Option<DropReason> {
    match &r.t.disposition {
        Disposition::Dropped { reason, .. } => Some(*reason),
        _ => None,
    }
}

fn open_panel(p: &ProgramIr, u: UiState, who: &str) -> StepResult {
    run(
        p,
        u,
        ui("open-panel", "page:1", serde_json::json!({ "p": who }), 1),
    )
}

// ── the commit / abort table ────────────────────────────────────────────────

struct TxnCase {
    name: &'static str,
    /// (emit, scope, payload) — dispatched onto the freshly booted state
    /// (page cases) or a freshly opened panel (surface cases).
    surface: bool,
    emit: &'static str,
    payload: serde_json::Value,
    expect: TxnExpect,
}

enum TxnExpect {
    /// (state assertions, pending count, tag counter, commands emitted)
    Commit {
        writes: usize,
        pending: usize,
        tag: u64,
        commands: usize,
    },
    /// `projection-not-ready`: nothing commits, counters roll back.
    Abort,
    /// No guard satisfied — dropped, guards recorded.
    NoHandler,
}

#[test]
fn the_commit_abort_table() {
    let p = program();
    let cases = [
        TxnCase {
            name: "set-field commits from page scope",
            surface: false,
            emit: "bump",
            payload: serde_json::json!({}),
            expect: TxnExpect::Commit {
                writes: 1,
                pending: 0,
                tag: 0,
                commands: 0,
            },
        },
        TxnCase {
            name: "send + tag bind + map write commit together",
            surface: false,
            emit: "send-ping",
            payload: serde_json::json!({ "x": 7 }),
            expect: TxnExpect::Commit {
                writes: 1,
                pending: 1,
                tag: 1,
                commands: 1,
            },
        },
        TxnCase {
            name: "guard-position not-ready reads as false",
            surface: false,
            emit: "bump-if-ready",
            payload: serde_json::json!({}),
            expect: TxnExpect::NoHandler,
        },
        TxnCase {
            name: "body not-ready aborts atomically after write and send",
            surface: false,
            emit: "write-then-stall",
            payload: serde_json::json!({}),
            expect: TxnExpect::Abort,
        },
        TxnCase {
            name: "set-field commits from surface scope",
            surface: true,
            emit: "panel-bump",
            payload: serde_json::json!({}),
            expect: TxnExpect::Commit {
                writes: 1,
                pending: 0,
                tag: 0,
                commands: 0,
            },
        },
        TxnCase {
            name: "send commits from surface scope",
            surface: true,
            emit: "panel-send",
            payload: serde_json::json!({}),
            expect: TxnExpect::Commit {
                writes: 0,
                pending: 1,
                tag: 1,
                commands: 1,
            },
        },
        TxnCase {
            name: "body not-ready aborts from surface scope",
            surface: true,
            emit: "panel-stall",
            payload: serde_json::json!({}),
            expect: TxnExpect::Abort,
        },
    ];

    for case in cases {
        let u0 = if case.surface {
            open_panel(&p, booted(&p), "a").u
        } else {
            booted(&p)
        };
        let scope = if case.surface { "surface:1" } else { "page:1" };
        let tag_before = u0.counters.tag;
        let rev_before = u0.rev;
        let state_before = if case.surface {
            u0.surfaces.last().unwrap().state.clone()
        } else {
            u0.nav.last().unwrap().state.clone()
        };
        let r = run(
            &p,
            u0,
            ui(case.emit, scope, case.payload.clone(), rev_before),
        );

        assert_eq!(r.u.rev, rev_before + 1, "{}: rev+1 always", case.name);
        assert_eq!(r.v.revision, r.u.rev, "{}: v.revision == u.rev", case.name);

        let Disposition::Dispatched(record) = &r.t.disposition else {
            panic!("{}: expected a dispatch record", case.name);
        };
        let state_after = if case.surface {
            r.u.surfaces.last().unwrap().state.clone()
        } else {
            r.u.nav.last().unwrap().state.clone()
        };
        match case.expect {
            TxnExpect::Commit {
                writes,
                pending,
                tag,
                commands,
            } => {
                assert!(record.aborted.is_none(), "{}", case.name);
                assert_eq!(record.writes.len(), writes, "{}: writes", case.name);
                assert_eq!(r.u.pending.len(), pending, "{}: pending", case.name);
                assert_eq!(
                    r.u.counters.tag,
                    tag_before + tag,
                    "{}: tag counter",
                    case.name
                );
                assert_eq!(r.c.len(), commands, "{}: commands", case.name);
            }
            TxnExpect::Abort => {
                assert_eq!(
                    record.aborted.as_deref(),
                    Some("projection-not-ready"),
                    "{}",
                    case.name
                );
                assert_eq!(state_after, state_before, "{}: no writes commit", case.name);
                assert!(record.writes.is_empty(), "{}: no writes traced", case.name);
                assert!(r.u.pending.is_empty(), "{}: no pending", case.name);
                assert!(r.c.is_empty(), "{}: no commands", case.name);
                assert_eq!(
                    r.u.counters.tag, tag_before,
                    "{}: counters roll back — replay ids never drift",
                    case.name
                );
            }
            TxnExpect::NoHandler => {
                assert_eq!(record.selected, None, "{}", case.name);
                assert_eq!(
                    record.guards.first().map(|g| g.result),
                    Some(GuardResult::NotReady),
                    "{}: the not-ready guard is traced",
                    case.name
                );
                assert_eq!(state_after, state_before, "{}", case.name);
            }
        }
    }
}

// ── acceptance (§7.2, in order) ─────────────────────────────────────────────

#[test]
fn acceptance_stale_scope_occluded_ineligible() {
    let p = program();

    // Unknown page serial → stale-scope.
    let r = run(
        &p,
        booted(&p),
        ui("bump", "page:9", serde_json::json!({}), 1),
    );
    assert_eq!(drop_reason(&r), Some(DropReason::StaleScope));
    assert_eq!(r.u.rev, 2, "rev+1 on drop");

    // A below-top page is not the interactive layer → stale-scope.
    let pushed = run(
        &p,
        booted(&p),
        ui("nav-other", "page:1", serde_json::json!({}), 1),
    )
    .u;
    let r = run(&p, pushed, ui("bump", "page:1", serde_json::json!({}), 2));
    assert_eq!(drop_reason(&r), Some(DropReason::StaleScope));

    // Page event under an open surface → occluded.
    let covered = open_panel(&p, booted(&p), "a").u;
    let r = run(
        &p,
        covered.clone(),
        ui("bump", "page:1", serde_json::json!({}), 2),
    );
    assert_eq!(drop_reason(&r), Some(DropReason::Occluded));

    // A non-top surface is occluded too (a page event can't open the
    // second instance — that path is itself occluded — so stack it
    // directly; the machine still owns the acceptance rule).
    let mut two = covered.clone();
    let serial = two.counters.mint_surface();
    two.surfaces.push(uhura_core::state::SurfaceState {
        serial,
        definition: ident("panel"),
        props: BTreeMap::from([(ident("who"), Value::Id("b".into()))]),
        state: BTreeMap::from([(ident("c"), Value::Int(0))]),
        opener: "page:1".into(),
        restore_focus: None,
    });
    let r = run(
        &p,
        two,
        ui("panel-bump", "surface:1", serde_json::json!({}), 3),
    );
    assert_eq!(drop_reason(&r), Some(DropReason::Occluded));

    // Undeclared event / payload-shape mismatch → ineligible.
    let r = run(
        &p,
        booted(&p),
        ui("nope", "page:1", serde_json::json!({}), 1),
    );
    assert_eq!(drop_reason(&r), Some(DropReason::Ineligible));
    let r = run(
        &p,
        booted(&p),
        ui("bump", "page:1", serde_json::json!({ "extra": 1 }), 1),
    );
    assert_eq!(drop_reason(&r), Some(DropReason::Ineligible));
    let r = run(
        &p,
        booted(&p),
        ui("send-ping", "page:1", serde_json::json!({}), 1),
    );
    assert_eq!(drop_reason(&r), Some(DropReason::Ineligible));

    // Stale view_rev is ACCEPTED — descriptors are self-contained (§7.2).
    let mut aged = booted(&p);
    aged.rev = 40;
    let r = run(&p, aged, ui("bump", "page:1", serde_json::json!({}), 3));
    assert_eq!(page_state(&r.u, "n"), &Value::Int(1));

    // The top surface's own events pass.
    let r = run(
        &p,
        covered,
        ui("panel-bump", "surface:1", serde_json::json!({}), 2),
    );
    let Disposition::Dispatched(record) = &r.t.disposition else {
        panic!("top-surface event dispatches");
    };
    assert_eq!(record.selected, Some(4), "panel-bump is the fifth handler");
}

// ── multi-handler selection ─────────────────────────────────────────────────

#[test]
fn multi_handler_first_satisfied_guard_wins() {
    let p = program();
    let r = run(
        &p,
        booted(&p),
        ui("multi", "page:1", serde_json::json!({ "x": 2 }), 1),
    );
    let Disposition::Dispatched(record) = &r.t.disposition else {
        panic!("dispatched");
    };
    assert_eq!(
        record
            .guards
            .iter()
            .map(|g| (g.handler, g.result))
            .collect::<Vec<_>>(),
        vec![(9, GuardResult::Unsatisfied), (10, GuardResult::Satisfied)],
    );
    assert_eq!(record.selected, Some(10));
    assert_eq!(page_state(&r.u, "n"), &Value::Int(2));

    let r = run(
        &p,
        booted(&p),
        ui("multi", "page:1", serde_json::json!({ "x": 3 }), 1),
    );
    let Disposition::Dispatched(record) = &r.t.disposition else {
        panic!("consulted");
    };
    assert_eq!(record.selected, None, "none satisfied ⇒ dropped");
    assert_eq!(page_state(&r.u, "n"), &Value::Int(0));
}

#[test]
fn writes_are_visible_sequentially_within_the_handler() {
    let p = program();
    let r = run(
        &p,
        booted(&p),
        ui("seq", "page:1", serde_json::json!({}), 1),
    );
    assert_eq!(page_state(&r.u, "n"), &Value::Int(2), "1 then 1+1");
}

// ── outcomes ────────────────────────────────────────────────────────────────

fn with_pending(p: &ProgramIr) -> UiState {
    run(
        p,
        booted(p),
        ui("send-ping", "page:1", serde_json::json!({ "x": 7 }), 1),
    )
    .u
}

#[test]
fn outcome_ok_dispatches_to_origin_with_echoed_payload() {
    let p = program();
    let u = with_pending(&p);
    assert_eq!(u.pending.len(), 1);
    let r = run(
        &p,
        u,
        Event::Outcome {
            correlation: "c-1".into(),
            result: OutcomeResult::Ok,
            updates: vec![],
        },
    );
    assert!(r.u.pending.is_empty(), "settled");
    assert_eq!(
        page_state(&r.u, "n"),
        &Value::Int(7),
        "cmd echoes the sent payload"
    );
    let Value::Record(m) = page_state(&r.u, "m") else {
        panic!()
    };
    assert!(m.is_empty(), "`set m[tag] = none` removes the entry");
}

#[test]
fn refused_and_unavailable_both_route_to_err() {
    let p = program();
    for (result, expected_note) in [
        (
            OutcomeResult::Refused {
                refusal: ident("rate-limited"),
            },
            "rate-limited",
        ),
        (
            OutcomeResult::Unavailable {
                reason: "unreachable".into(),
            },
            "unavailable",
        ),
    ] {
        let r = run(
            &p,
            with_pending(&p),
            Event::Outcome {
                correlation: "c-1".into(),
                result,
                updates: vec![],
            },
        );
        assert_eq!(page_state(&r.u, "note"), &Value::Text(expected_note.into()));
    }
}

#[test]
fn unknown_correlation_and_unmounted_origin_drop() {
    let p = program();
    let r = run(
        &p,
        booted(&p),
        Event::Outcome {
            correlation: "c-99".into(),
            result: OutcomeResult::Ok,
            updates: vec![],
        },
    );
    assert_eq!(drop_reason(&r), Some(DropReason::UnknownCorrelation));

    // Origin scope gone (its surface dismissed) ⇒ stale-outcome, pending
    // removed — truth already lives in X (§7.2).
    let opened = open_panel(&p, booted(&p), "a").u;
    let sent = run(
        &p,
        opened,
        ui("panel-send", "surface:1", serde_json::json!({}), 2),
    )
    .u;
    let closed = run(&p, sent, ui("close", "surface:1", serde_json::json!({}), 3)).u;
    assert_eq!(closed.pending.len(), 1);
    let r = run(
        &p,
        closed,
        Event::Outcome {
            correlation: "c-1".into(),
            result: OutcomeResult::Ok,
            updates: vec![],
        },
    );
    assert_eq!(drop_reason(&r), Some(DropReason::StaleOutcome));
    assert!(r.u.pending.is_empty(), "pending removed on stale drop");
}

#[test]
fn outcomes_reach_below_top_pages() {
    // Navigate away while a command is in flight: the origin page is still
    // mounted (revealed pages keep state), so the outcome dispatches.
    let p = program();
    let u = with_pending(&p);
    let pushed = run(&p, u, ui("nav-other", "page:1", serde_json::json!({}), 2)).u;
    let r = run(
        &p,
        pushed,
        Event::Outcome {
            correlation: "c-1".into(),
            result: OutcomeResult::Ok,
            updates: vec![],
        },
    );
    assert!(r.u.pending.is_empty());
    assert_eq!(&r.u.nav[0].state[&ident("n")], &Value::Int(7));
}

// ── structure ───────────────────────────────────────────────────────────────

#[test]
fn open_surface_is_idempotent_per_definition_and_context() {
    let p = program();
    let once = open_panel(&p, booted(&p), "a");
    assert_eq!(once.u.surfaces.len(), 1);
    assert_eq!(once.u.surfaces[0].serial, 1);
    assert_eq!(once.u.surfaces[0].opener, "page:1");

    // Same (definition, canonical context) ⇒ no second instance. The
    // event still dispatches from the page… which is occluded now, so
    // drive it through the surface-free path: dismiss first, reopen.
    let closed = run(
        &p,
        once.u,
        Event::Ui {
            descriptor: Descriptor {
                kind: DescriptorKind::Input,
                event: ident("dismiss"),
                emit: ident("dismiss"),
                scope: "surface:1".into(),
                payload: serde_json::json!({}),
                carries: BTreeMap::new(),
            },
            data: None,
            view_rev: 2,
        },
    );
    assert!(closed.u.surfaces.is_empty(), "reserved dismiss pops");

    let reopened = open_panel(&p, closed.u, "a");
    assert_eq!(
        reopened.u.surfaces[0].serial, 2,
        "a fresh instance mints a fresh serial"
    );
    let different = open_panel(&p, booted(&p), "b");
    assert_eq!(
        different.u.surfaces[0].props[&ident("who")],
        Value::Id("b".into())
    );
}

#[test]
fn open_records_the_triggering_node_and_dismiss_restores_focus() {
    let p = program();
    // The home markup has exactly this control: button ord 1, press ⇒
    // open-panel(p: "a"). The descriptor search records its key-path.
    let opened = open_panel(&p, booted(&p), "a");
    assert_eq!(
        opened.u.surfaces[0].restore_focus.as_deref(),
        Some("page:1/0/1"),
        "the pre-step view knows the triggering node"
    );
    assert_eq!(
        opened.v.surfaces[0].restore_focus.as_deref(),
        Some("page:1/0/1"),
        "V carries it for the renderer"
    );

    // Authored `dismiss` statement pops the topmost instance ⇒
    // FocusRestore intent with that key-path.
    let closed = run(
        &p,
        opened.u,
        ui("close", "surface:1", serde_json::json!({}), 2),
    );
    assert!(closed.u.surfaces.is_empty());
    assert!(
        closed
            .i
            .iter()
            .any(|i| i.to_json()["intent"] == "focus-restore"
                && i.to_json()["key-path"] == "page:1/0/1"),
        "FocusRestore intent: {:?}",
        closed.i
    );
}

#[test]
fn navigate_pushes_and_back_pops_with_history_intents() {
    let p = program();
    let bumped = run(
        &p,
        booted(&p),
        ui("bump", "page:1", serde_json::json!({}), 1),
    )
    .u;
    let pushed = run(
        &p,
        bumped,
        ui("nav-other", "page:1", serde_json::json!({}), 2),
    );
    assert_eq!(pushed.u.nav.len(), 2);
    assert_eq!(pushed.u.nav[1].route, ident("other"));
    assert_eq!(pushed.u.nav[1].serial, 2, "page serials mint 1-based");
    assert!(
        pushed
            .i
            .iter()
            .any(|i| i.to_json()["intent"] == "history-push"),
        "{:?}",
        pushed.i
    );

    // `other` has no nav-back handler; drive back from home by crafting
    // the pop through the machine: dispatch nav-back on the revealed page
    // is stale-scope, so pop via a fresh home-on-top stack instead.
    let mut stacked = pushed.u.clone();
    stacked.nav.swap(0, 1); // home on top, other revealed
    let back = run(
        &p,
        stacked,
        ui("nav-back", "page:1", serde_json::json!({}), 3),
    );
    assert_eq!(back.u.nav.len(), 1);
    assert_eq!(back.u.nav[0].route, ident("other"));
    assert!(
        back.i
            .iter()
            .any(|i| i.to_json()["intent"] == "history-back"),
        "{:?}",
        back.i
    );
}

#[test]
fn back_force_closes_the_popped_pages_surfaces_and_never_underflows() {
    let p = program();
    // A page under an open surface can't receive `nav-back` (occluded),
    // so back-with-surfaces only happens from a SURFACE handler running
    // `navigate back`: craft that stack — panel opened by the top page —
    // and drive the pop through the surface.
    let opened = open_panel(&p, booted(&p), "a").u;
    let mut u = opened;
    u.nav.push(uhura_core::state::NavEntry {
        serial: u.counters.mint_page(),
        route: ident("home"),
        params: BTreeMap::new(),
        state: BTreeMap::from([
            (ident("n"), Value::Int(0)),
            (ident("m"), Value::Record(BTreeMap::new())),
            (ident("note"), Value::None),
        ]),
    });
    u.surfaces[0].opener = "page:2".to_string(); // opened by the top page
    let r = run(
        &p,
        u,
        ui("panel-back", "surface:1", serde_json::json!({}), 5),
    );
    assert_eq!(r.u.nav.len(), 1);
    assert!(
        r.u.surfaces.is_empty(),
        "the popped page's surfaces force-close"
    );

    // Popping the last entry is a traced no-op, never a crash.
    let r2 = run(&p, r.u, ui("nav-back", "page:1", serde_json::json!({}), 6));
    assert_eq!(r2.u.nav.len(), 1);
    assert!(
        r2.t.structural.iter().any(|op| op["op"] == "nav-underflow"),
        "{:?}",
        r2.t.structural
    );
}

// ── deliveries ──────────────────────────────────────────────────────────────

#[test]
fn projection_events_recompute_the_view_without_dispatch() {
    let p = program();
    let u = booted(&p);
    let rev = u.rev;
    let r = run(&p, u, Event::Projection { updates: vec![] });
    assert!(matches!(r.t.disposition, Disposition::Delivery));
    assert_eq!(r.u.rev, rev + 1);
    assert_eq!(r.v.revision, r.u.rev);
}

#[test]
fn init_mounts_the_route_and_hashes_are_stable() {
    let p = program();
    let r = step_u(
        &p,
        UiState::boot(),
        &projections(),
        Event::Init {
            route: ident("home"),
            params: BTreeMap::new(),
        },
    )
    .expect("init");
    assert_eq!(r.u.rev, 1);
    assert_eq!(r.u.nav.len(), 1);
    assert_eq!(r.u.nav[0].serial, 1);
    assert_eq!(r.t.u_hash, r.u.u_hash());
    assert_eq!(r.t.v_hash, r.v.v_hash());

    // Determinism: the same fold yields byte-identical traces.
    let again = step_u(
        &p,
        UiState::boot(),
        &projections(),
        Event::Init {
            route: ident("home"),
            params: BTreeMap::new(),
        },
    )
    .expect("init");
    assert_eq!(r.t.to_line(), again.t.to_line());
}

#[test]
fn a_second_open_with_the_same_context_is_idempotent() {
    // The §4.2 cell the name promises: same (definition, canonical
    // context) while mounted ⇒ no second instance, traced `already-open`.
    let p = program();
    let opened = open_panel(&p, booted(&p), "a").u;
    let r = run(
        &p,
        opened,
        ui("reopen", "surface:1", serde_json::json!({}), 2),
    );
    assert_eq!(r.u.surfaces.len(), 1, "no duplicate instance");
    assert_eq!(r.u.surfaces[0].serial, 1, "the existing instance survives");
    assert_eq!(
        r.u.counters.surface_serial, 1,
        "no serial minted for a dedup'd open"
    );
    assert!(
        r.t.structural.iter().any(|op| op["op"] == "already-open"),
        "{:?}",
        r.t.structural
    );
}

#[test]
fn the_focus_search_sees_the_view_the_event_was_emitted_against() {
    // flip-open writes state its OWN trigger payload reads before opening:
    // the pre-commit view still carries payload {c:false}, so the search
    // must find the button even though the post-commit payload is {c:true}
    // (§4.2 "records opener + triggering node").
    let p = program();
    let r = run(
        &p,
        booted(&p),
        ui("flip-open", "page:1", serde_json::json!({ "c": false }), 1),
    );
    assert_eq!(r.u.surfaces.len(), 1);
    assert_eq!(
        r.u.surfaces[0].restore_focus.as_deref(),
        Some("page:1/0/2"),
        "the trigger node is found in the pre-commit view"
    );
}

#[test]
fn a_surface_opened_by_a_scope_that_died_mid_dispatch_is_swept() {
    // `dismiss` then `open-surface` in one handler: the opener scope is
    // dead when the open applies, so the invariant sweep force-closes the
    // orphan — it must not float over the page forever.
    let p = program();
    let opened = open_panel(&p, booted(&p), "a").u;
    let r = run(
        &p,
        opened,
        ui("respawn", "surface:1", serde_json::json!({}), 2),
    );
    assert!(
        r.u.surfaces.is_empty(),
        "the orphan is swept: {:?}",
        r.u.surfaces
    );
    let ops: Vec<_> =
        r.t.structural
            .iter()
            .map(|op| op["op"].as_str().unwrap_or_default().to_string())
            .collect();
    assert_eq!(
        ops,
        vec!["dismiss", "open-surface", "force-close"],
        "dismissed, opened, then swept — all traced"
    );
}

#[test]
fn a_duplicate_identical_in_flight_send_warns_in_g() {
    // §4.2 send: "Duplicate identical in-flight send → warning" —
    // suppression is the author's guard's job; the machine diagnoses.
    let p = program();
    let once = with_pending(&p);
    let r = run(
        &p,
        once,
        ui("send-ping", "page:1", serde_json::json!({ "x": 7 }), 2),
    );
    assert_eq!(r.u.pending.len(), 2, "the duplicate still commits");
    assert_eq!(r.g.len(), 1, "one warning in G");
    assert_eq!(r.g[0].code, "UH8001");
    assert!(
        r.t.to_line().contains("UH8001"),
        "the warning reaches the trace record"
    );

    // A different payload is not a duplicate.
    let r2 = run(
        &p,
        with_pending(&p),
        ui("send-ping", "page:1", serde_json::json!({ "x": 8 }), 2),
    );
    assert!(r2.g.is_empty(), "{:?}", r2.g);
}
