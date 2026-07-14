//! Read-only inspection protocol. This is deliberately separate from the
//! frozen renderer/trace ABI: tooling may observe complete `U`/`X` state
//! without making either part of `StepResult` or the canonical JSONL trace.

use std::collections::{BTreeMap, BTreeSet};

use crate::event::ApplyNote;
use crate::ir::{self, ProgramIr};
use crate::state::{Projections, UiState};
use crate::view::Snapshot;

pub const INSPECT_PROTOCOL: &str = "uhura-inspect/0";

/// A complete committed-session snapshot for developer tooling. The caller
/// decides when to serialize it; the deterministic runtime never emits it as
/// an effect and does not depend on an inspector being present.
pub fn snapshot_json(
    program: &ProgramIr,
    u: &UiState,
    x: &Projections,
    v: Option<&Snapshot>,
    pending_applies: &[ApplyNote],
) -> serde_json::Value {
    // Full X can be large in developer sessions. Materialize each store once,
    // then hash and publish those exact bytes so identity cannot drift from
    // the payload a subscriber receives.
    let configuration = u.configuration_json();
    let configuration_hash = uhura_base::hash_json(&configuration);
    let mut u_json = configuration;
    u_json["rev"] = u.rev.into();
    let u_hash = uhura_base::hash_json(&u_json);
    let x_json = x.to_json();
    let x_hash = uhura_base::hash_json(&x_json);
    let view = v.map(|snapshot| {
        serde_json::json!({
            "revision": snapshot.revision,
            "route": snapshot.page.route.to_string(),
            "surface-count": snapshot.surfaces.len(),
            "v-hash": snapshot.v_hash(),
        })
    });
    serde_json::json!({
        "protocol": INSPECT_PROTOCOL,
        "kind": "snapshot",
        "ir-hash": program.hash(),
        "revision": u.rev,
        "configuration-hash": configuration_hash,
        "u-hash": u_hash,
        "x-hash": x_hash,
        "u": u_json,
        "x": x_json,
        "view": view,
        "pending-applies": pending_applies.iter().map(ApplyNote::to_json).collect::<Vec<_>>(),
    })
}

/// Static, renderer-independent behavior topology extracted from checked IR.
/// Node identifiers deliberately align definition/handler ids with the
/// compiler span-table keys and live trace `(definition, selected, on)` data.
pub fn program_graph(program: &ProgramIr) -> serde_json::Value {
    let mut nodes = BTreeMap::<String, serde_json::Value>::new();
    let mut edges = Vec::<serde_json::Value>::new();
    let mut settles = BTreeSet::<(String, String)>::new();

    for (name, projection) in &program.projections {
        let id = format!("projections.{name}");
        nodes.insert(
            id.clone(),
            serde_json::json!({
                "id": id,
                "kind": "projection",
                "name": name.to_string(),
                "port": projection.port.to_string(),
                "boot": projection.boot,
                "keyed": projection.key.is_some(),
            }),
        );
    }

    for (definition_kind, prefix, definitions) in [
        ("page", "pages", &program.pages),
        ("component", "components", &program.components),
        ("surface", "surfaces", &program.surfaces),
    ] {
        for (name, definition) in definitions {
            let definition_id = format!("{prefix}.{name}");
            let mut definition_node = serde_json::json!({
                "id": definition_id,
                "kind": "definition",
                "definition-kind": definition_kind,
                "name": name.to_string(),
            });
            if definition_kind == "page" {
                definition_node["entry"] = (name == &program.entry).into();
            }
            nodes.insert(definition_id.clone(), definition_node);

            for (field, initial) in &definition.state {
                let id = format!("{definition_id}/state/{field}");
                nodes.insert(
                    id.clone(),
                    serde_json::json!({
                        "id": id,
                        "kind": "state",
                        "definition": definition_id,
                        "name": field.to_string(),
                        "initial": crate::state::init_value(initial).to_json(),
                    }),
                );
            }

            for (index, handler) in definition.handlers.iter().enumerate() {
                add_handler(
                    &definition_id,
                    index,
                    handler,
                    &mut nodes,
                    &mut edges,
                    &mut settles,
                );
            }
        }
    }

    for (from, to) in settles {
        edges.push(serde_json::json!({ "kind": "settles", "from": from, "to": to }));
    }

    serde_json::json!({
        "protocol": INSPECT_PROTOCOL,
        "kind": "program",
        "ir": {
            "protocol": program.protocol,
            "hash": program.hash(),
            "app": program.app.to_string(),
            "entry": program.entry.to_string(),
        },
        "nodes": nodes.into_values().collect::<Vec<_>>(),
        "edges": edges,
        "span-offset-encoding": "utf-8-bytes",
        "spans": {},
    })
}

fn add_handler(
    definition_id: &str,
    index: usize,
    handler: &ir::HandlerIr,
    nodes: &mut BTreeMap<String, serde_json::Value>,
    edges: &mut Vec<serde_json::Value>,
    settles: &mut BTreeSet<(String, String)>,
) {
    let (on, event_kind, command, outcome) = match &handler.on {
        ir::EventKeyIr::Semantic { event } => (event.to_string(), "semantic", None, None),
        ir::EventKeyIr::Outcome { command, which } => {
            let outcome = match which {
                ir::OutcomeKindIr::Ok => "ok",
                ir::OutcomeKindIr::Err => "err",
            };
            (
                format!("{command}.{outcome}"),
                "outcome",
                Some(command.to_string()),
                Some(outcome),
            )
        }
    };
    let event_id = format!("{definition_id}/event/{on}");
    let mut event_node = serde_json::json!({
        "id": event_id,
        "kind": "event",
        "definition": definition_id,
        "name": on,
        "event-kind": event_kind,
    });
    if let Some(command) = &command {
        event_node["command"] = command.clone().into();
        event_node["outcome"] = outcome.expect("outcome event has a kind").into();
    }
    nodes.insert(event_id.clone(), event_node);

    let handler_id = format!("{definition_id}/handler/{index}");
    let effects = handler
        .body
        .iter()
        .filter_map(|statement| match statement {
            ir::StmtIr::Dismiss => Some("dismiss"),
            ir::StmtIr::NavigateBack => Some("back"),
            _ => None,
        })
        .collect::<Vec<_>>();
    nodes.insert(
        handler_id.clone(),
        serde_json::json!({
            "id": handler_id,
            "kind": "handler",
            "definition": definition_id,
            "index": index,
            "on": on,
            "guarded": handler.guard.is_some(),
            "effects": effects,
        }),
    );
    edges.push(serde_json::json!({
        "kind": "handles",
        "from": event_id,
        "to": handler_id,
    }));

    if let Some(command) = command {
        let command_id = format!("commands.{command}");
        nodes.entry(command_id.clone()).or_insert_with(|| {
            serde_json::json!({
                "id": command_id,
                "kind": "command",
                "name": command,
            })
        });
        settles.insert((command_id, event_id));
    }

    if let Some(guard) = &handler.guard {
        let mut reads = BTreeSet::new();
        collect_reads(guard, definition_id, &mut reads);
        add_read_edges("guard-reads", reads, &handler_id, edges);
    }

    let mut body_reads = BTreeSet::new();
    for (order, statement) in handler.body.iter().enumerate() {
        collect_statement_reads(statement, definition_id, &mut body_reads);
        match statement {
            ir::StmtIr::Set { field, .. } => edges.push(serde_json::json!({
                "kind": "writes",
                "from": handler_id,
                "to": format!("{definition_id}/state/{field}"),
                "order": order,
            })),
            ir::StmtIr::Send { port, command, .. } => {
                let command_id = format!("commands.{command}");
                nodes.insert(
                    command_id.clone(),
                    serde_json::json!({
                        "id": command_id,
                        "kind": "command",
                        "name": command.to_string(),
                        "port": port.to_string(),
                    }),
                );
                edges.push(serde_json::json!({
                    "kind": "sends",
                    "from": handler_id,
                    "to": command_id,
                    "order": order,
                }));
            }
            ir::StmtIr::OpenSurface { surface, .. } => edges.push(serde_json::json!({
                "kind": "opens",
                "from": handler_id,
                "to": format!("surfaces.{surface}"),
                "order": order,
            })),
            ir::StmtIr::Navigate { route, .. } => edges.push(serde_json::json!({
                "kind": "navigates",
                "from": handler_id,
                "to": format!("pages.{route}"),
                "order": order,
                "mode": "push",
            })),
            ir::StmtIr::NavigateReplace { route, .. } => edges.push(serde_json::json!({
                "kind": "navigates",
                "from": handler_id,
                "to": format!("pages.{route}"),
                "order": order,
                "mode": "replace",
            })),
            ir::StmtIr::Dismiss | ir::StmtIr::NavigateBack => {}
        }
    }
    add_read_edges("body-reads", body_reads, &handler_id, edges);
}

fn add_read_edges(
    kind: &str,
    reads: BTreeSet<String>,
    handler_id: &str,
    edges: &mut Vec<serde_json::Value>,
) {
    for source in reads {
        edges.push(serde_json::json!({ "kind": kind, "from": source, "to": handler_id }));
    }
}

fn collect_statement_reads(
    statement: &ir::StmtIr,
    definition_id: &str,
    reads: &mut BTreeSet<String>,
) {
    match statement {
        ir::StmtIr::Set { key, value, .. } => {
            if let Some(key) = key {
                collect_reads(key, definition_id, reads);
            }
            collect_reads(value, definition_id, reads);
        }
        ir::StmtIr::Send { args, .. }
        | ir::StmtIr::OpenSurface { args, .. }
        | ir::StmtIr::Navigate { args, .. }
        | ir::StmtIr::NavigateReplace { args, .. } => {
            for arg in args {
                collect_reads(&arg.value, definition_id, reads);
            }
        }
        ir::StmtIr::Dismiss | ir::StmtIr::NavigateBack => {}
    }
}

fn collect_reads(expr: &ir::ExprIr, definition_id: &str, reads: &mut BTreeSet<String>) {
    match expr {
        ir::ExprIr::StateRef(field) => {
            reads.insert(format!("{definition_id}/state/{field}"));
        }
        ir::ExprIr::ProjectionRef(projection) => {
            reads.insert(format!("projections.{projection}"));
        }
        ir::ExprIr::ProjectionKeyed { projection, key } => {
            reads.insert(format!("projections.{projection}"));
            collect_reads(key, definition_id, reads);
        }
        ir::ExprIr::Field { base, .. }
        | ir::ExprIr::Unary { expr: base, .. }
        | ir::ExprIr::ToText(base)
        | ir::ExprIr::Count(base) => collect_reads(base, definition_id, reads),
        ir::ExprIr::Index { base, key }
        | ir::ExprIr::Binary {
            lhs: base,
            rhs: key,
            ..
        } => {
            collect_reads(base, definition_id, reads);
            collect_reads(key, definition_id, reads);
        }
        ir::ExprIr::If { cond, then, els } => {
            collect_reads(cond, definition_id, reads);
            collect_reads(then, definition_id, reads);
            collect_reads(els, definition_id, reads);
        }
        ir::ExprIr::RecordLit(args) => {
            for arg in args {
                collect_reads(&arg.value, definition_id, reads);
            }
        }
        ir::ExprIr::Int(_)
        | ir::ExprIr::Text(_)
        | ir::ExprIr::Bool(_)
        | ir::ExprIr::None
        | ir::ExprIr::PropRef(_)
        | ir::ExprIr::ParamRef(_)
        | ir::ExprIr::BindingRef(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_base::{Ident, Value};

    #[test]
    fn boot_snapshot_is_versioned_complete_and_revision_independent() {
        let mut u = UiState::boot();
        let mut x = Projections::default();
        x.failed.insert(
            (
                Ident::new("feed-page").unwrap(),
                Some(Value::Id("user-1".into())),
            ),
            "offline".into(),
        );

        let program = empty_program();
        let before = snapshot_json(&program, &u, &x, None, &[]);
        let configuration_hash = before["configuration-hash"].clone();
        u.rev += 1;
        let after = snapshot_json(&program, &u, &x, None, &[]);

        assert_eq!(before["protocol"], INSPECT_PROTOCOL);
        assert_eq!(before["kind"], "snapshot");
        assert_eq!(before["revision"], 0);
        assert_eq!(before["u"]["rev"], 0);
        assert_eq!(before["x"]["failed"][0]["reason"], "offline");
        assert!(before["view"].is_null());
        assert_eq!(after["configuration-hash"], configuration_hash);
        assert_eq!(after["revision"], 1);
        assert_eq!(after["u"]["rev"], 1);
    }

    #[test]
    fn program_graph_pins_runtime_values_stable_ids_and_behavior_edges() {
        let program = graph_program();
        let graph = program_graph(&program);

        assert_eq!(graph["protocol"], INSPECT_PROTOCOL);
        assert_eq!(graph["kind"], "program");
        assert_eq!(graph["ir"]["protocol"], ir::IR_PROTOCOL);
        assert_eq!(graph["ir"]["hash"], program.hash());
        assert_eq!(graph["ir"]["app"], "test-app");
        assert_eq!(graph["ir"]["entry"], "home");
        assert_eq!(graph["span-offset-encoding"], "utf-8-bytes");
        assert_eq!(graph["spans"], serde_json::json!({}));

        let node_ids = graph["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .map(|node| node["id"].as_str().expect("node id"))
            .collect::<Vec<_>>();
        assert_eq!(
            node_ids,
            [
                "commands.save",
                "pages.home",
                "pages.home/event/save.ok",
                "pages.home/event/submit",
                "pages.home/handler/0",
                "pages.home/handler/1",
                "pages.home/state/pending",
                "projections.feed-page",
            ]
        );

        let node = |id: &str| {
            graph["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .find(|node| node["id"] == id)
                .unwrap_or_else(|| panic!("missing node `{id}`"))
        };
        assert_eq!(node("pages.home")["entry"], true);
        assert_eq!(node("pages.home/state/pending")["initial"], false);
        assert_eq!(node("projections.feed-page")["port"], "feed");
        assert_eq!(node("commands.save")["port"], "feed");

        let edges = graph["edges"].as_array().expect("edges");
        for expected in [
            serde_json::json!({
                "kind": "handles",
                "from": "pages.home/event/submit",
                "to": "pages.home/handler/0",
            }),
            serde_json::json!({
                "kind": "guard-reads",
                "from": "pages.home/state/pending",
                "to": "pages.home/handler/0",
            }),
            serde_json::json!({
                "kind": "body-reads",
                "from": "projections.feed-page",
                "to": "pages.home/handler/0",
            }),
            serde_json::json!({
                "kind": "writes",
                "from": "pages.home/handler/0",
                "to": "pages.home/state/pending",
                "order": 0,
            }),
            serde_json::json!({
                "kind": "sends",
                "from": "pages.home/handler/0",
                "to": "commands.save",
                "order": 1,
            }),
            serde_json::json!({
                "kind": "settles",
                "from": "commands.save",
                "to": "pages.home/event/save.ok",
            }),
        ] {
            assert!(edges.contains(&expected), "missing edge {expected}");
        }
    }

    fn graph_program() -> ProgramIr {
        let mut program = empty_program();
        program.projections.insert(
            Ident::new("feed-page").unwrap(),
            ir::ProjectionIr {
                port: Ident::new("feed").unwrap(),
                boot: true,
                ty: ir::TyIr::Bool,
                key: None,
            },
        );

        let pending = Ident::new("pending").unwrap();
        program.pages.insert(
            Ident::new("home").unwrap(),
            ir::DefIr {
                modality: None,
                props: vec![],
                emits: vec![],
                params: vec![],
                state: BTreeMap::from([(pending.clone(), ir::InitValue::Bool(false))]),
                events: BTreeMap::from([(Ident::new("submit").unwrap(), vec![])]),
                handlers: vec![
                    ir::HandlerIr {
                        on: ir::EventKeyIr::Semantic {
                            event: Ident::new("submit").unwrap(),
                        },
                        params: vec![],
                        guard: Some(ir::ExprIr::StateRef(pending.clone())),
                        body: vec![
                            ir::StmtIr::Set {
                                field: pending.clone(),
                                key: None,
                                value: ir::ExprIr::Bool(true),
                            },
                            ir::StmtIr::Send {
                                port: Ident::new("feed").unwrap(),
                                command: Ident::new("save").unwrap(),
                                args: vec![ir::ArgIr {
                                    name: Ident::new("snapshot").unwrap(),
                                    value: ir::ExprIr::ProjectionRef(
                                        Ident::new("feed-page").unwrap(),
                                    ),
                                }],
                                bind: None,
                            },
                        ],
                    },
                    ir::HandlerIr {
                        on: ir::EventKeyIr::Outcome {
                            command: Ident::new("save").unwrap(),
                            which: ir::OutcomeKindIr::Ok,
                        },
                        params: vec![],
                        guard: None,
                        body: vec![ir::StmtIr::Set {
                            field: pending,
                            key: None,
                            value: ir::ExprIr::Bool(false),
                        }],
                    },
                ],
                root: ir::NodeIr::Element(ir::ElementIr {
                    element: Ident::new("view").unwrap(),
                    ord: 0,
                    class: None,
                    props: vec![],
                    events: vec![],
                    text: vec![],
                    children: vec![],
                }),
            },
        );
        program
    }

    fn empty_program() -> ProgramIr {
        ProgramIr {
            protocol: ir::IR_PROTOCOL.into(),
            app: Ident::new("test-app").unwrap(),
            entry: Ident::new("home").unwrap(),
            catalog: ir::CatalogPin {
                name: Ident::new("base").unwrap(),
                version: "0.1.0".into(),
                hash: "catalog-hash".into(),
            },
            ports: BTreeMap::new(),
            projections: BTreeMap::new(),
            element_events: BTreeMap::new(),
            element_props: BTreeMap::new(),
            routes: BTreeMap::new(),
            pages: BTreeMap::new(),
            components: BTreeMap::new(),
            surfaces: BTreeMap::new(),
        }
    }
}
