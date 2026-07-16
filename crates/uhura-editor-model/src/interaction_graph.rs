//! A deterministic, renderer-neutral interaction graph derived from checked
//! Uhura IR. This is a projection of semantics Uhura already owns, never a
//! second interpreter: handler events/guards and resolved statements become
//! typed nodes and edges for NCC or any other read-only visualizer.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use uhura_core::ir::{DefIr, EventKeyIr, OutcomeKindIr, ProgramIr, StmtIr};

pub const INTERACTION_GRAPH_PROTOCOL: &str = "uhura-interaction-graph/0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct InteractionGraph {
    pub protocol: String,
    pub app: String,
    pub entry: String,
    pub nodes: Vec<InteractionNode>,
    pub edges: Vec<InteractionEdge>,
}

impl Default for InteractionGraph {
    /// An empty graph that still speaks the protocol, for hosts that need a
    /// placeholder render before any checked program exists.
    fn default() -> Self {
        Self {
            protocol: INTERACTION_GRAPH_PROTOCOL.to_string(),
            app: String::new(),
            entry: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct InteractionNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modality: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Page,
    Surface,
    Command,
    Dynamic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct InteractionEdge {
    pub id: String,
    pub kind: EdgeKind,
    pub from: String,
    pub to: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    Navigate,
    NavigateBack,
    Present,
    Dismiss,
    StateChange,
    SendCommand,
    ReceiveOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok,
    Err,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SourceRef {
    pub file: String,
    pub start: u32,
    pub end: u32,
    pub ir_path: String,
}

/// Minimal span shape accepted from `uhura-check` without making this pure
/// projection crate depend on the checker crate (which would invert layers).
pub trait SpanLookup {
    fn source_ref(&self, ir_path: &str) -> Option<SourceRef>;
}

impl SpanLookup for BTreeMap<String, SourceRef> {
    fn source_ref(&self, ir_path: &str) -> Option<SourceRef> {
        self.get(ir_path).cloned()
    }
}

struct NoSpans;
impl SpanLookup for NoSpans {
    fn source_ref(&self, _ir_path: &str) -> Option<SourceRef> {
        None
    }
}

/// Build a graph without source locations, useful for consumers that only
/// persisted `ir.json`. The CLI uses `build_interaction_graph_with_spans`.
pub fn build_interaction_graph(program: &ProgramIr) -> InteractionGraph {
    build_interaction_graph_with_spans(program, &NoSpans)
}

pub fn build_interaction_graph_with_spans(
    program: &ProgramIr,
    spans: &impl SpanLookup,
) -> InteractionGraph {
    let mut nodes = BTreeMap::<String, InteractionNode>::new();
    let mut edges = Vec::new();
    let mut command_ports = BTreeMap::<String, String>::new();

    for (name, def) in &program.pages {
        insert_node(
            &mut nodes,
            page_id(name.as_str()),
            NodeKind::Page,
            name.as_str(),
            None,
        );
        discover_commands(def, &mut command_ports);
    }
    for (name, def) in &program.surfaces {
        insert_node(
            &mut nodes,
            surface_id(name.as_str()),
            NodeKind::Surface,
            name.as_str(),
            def.modality.clone(),
        );
        discover_commands(def, &mut command_ports);
    }

    insert_node(
        &mut nodes,
        "dynamic:opener".into(),
        NodeKind::Dynamic,
        "surface opener",
        None,
    );
    insert_node(
        &mut nodes,
        "dynamic:previous-page".into(),
        NodeKind::Dynamic,
        "previous page",
        None,
    );
    for (command, port) in &command_ports {
        let label = format!("{port}.{command}");
        insert_node(
            &mut nodes,
            command_id(&label),
            NodeKind::Command,
            &label,
            None,
        );
    }

    for (name, def) in &program.pages {
        emit_def_edges(
            "pages",
            name.as_str(),
            def,
            &page_id(name.as_str()),
            spans,
            &command_ports,
            &mut nodes,
            &mut edges,
        );
    }
    for (name, def) in &program.surfaces {
        emit_def_edges(
            "surfaces",
            name.as_str(),
            def,
            &surface_id(name.as_str()),
            spans,
            &command_ports,
            &mut nodes,
            &mut edges,
        );
    }

    InteractionGraph {
        protocol: INTERACTION_GRAPH_PROTOCOL.into(),
        app: program.app.to_string(),
        entry: page_id(program.entry.as_str()),
        nodes: nodes.into_values().collect(),
        edges,
    }
}

fn discover_commands(def: &DefIr, commands: &mut BTreeMap<String, String>) {
    for handler in &def.handlers {
        for stmt in &handler.body {
            if let StmtIr::Send { port, command, .. } = stmt {
                commands.insert(command.to_string(), port.to_string());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_def_edges(
    prefix: &str,
    name: &str,
    def: &DefIr,
    owner: &str,
    spans: &impl SpanLookup,
    command_ports: &BTreeMap<String, String>,
    nodes: &mut BTreeMap<String, InteractionNode>,
    edges: &mut Vec<InteractionEdge>,
) {
    for (handler_index, handler) in def.handlers.iter().enumerate() {
        let event = event_label(&handler.on);
        let source_path = format!("{prefix}.{name}/handler/{handler_index}");
        let source = spans.source_ref(&source_path);
        let guard = handler
            .guard
            .as_ref()
            .map(|g| serde_json::to_value(g).expect("checked expressions serialize"));

        if let EventKeyIr::Outcome { command, which } = &handler.on {
            let command_name = command.to_string();
            let qualified = command_ports
                .get(&command_name)
                .map(|port| format!("{port}.{command_name}"))
                .unwrap_or_else(|| command_name.clone());
            let command_node = command_id(&qualified);
            insert_node(
                nodes,
                command_node.clone(),
                NodeKind::Command,
                &qualified,
                None,
            );
            edges.push(InteractionEdge {
                id: edge_id(prefix, name, handler_index, "outcome", 0),
                kind: EdgeKind::ReceiveOutcome,
                from: command_node,
                to: owner.to_string(),
                event: event.clone(),
                guard: guard.clone(),
                command: Some(qualified),
                outcome: Some(match which {
                    OutcomeKindIr::Ok => Outcome::Ok,
                    OutcomeKindIr::Err => Outcome::Err,
                }),
                source: source.clone(),
            });
        }

        let mut emitted_state_fields = BTreeSet::new();
        for (stmt_index, stmt) in handler.body.iter().enumerate() {
            let (kind, to, command) = match stmt {
                StmtIr::Set { field, .. } => {
                    if !emitted_state_fields.insert(field.to_string()) {
                        continue;
                    }
                    (EdgeKind::StateChange, owner.to_string(), None)
                }
                StmtIr::Send { port, command, .. } => {
                    let qualified = format!("{port}.{command}");
                    let id = command_id(&qualified);
                    insert_node(nodes, id.clone(), NodeKind::Command, &qualified, None);
                    (EdgeKind::SendCommand, id, Some(qualified))
                }
                StmtIr::OpenSurface { surface, .. } => {
                    (EdgeKind::Present, surface_id(surface.as_str()), None)
                }
                StmtIr::Dismiss => (EdgeKind::Dismiss, "dynamic:opener".into(), None),
                // A replace-navigation reaches the same page as a push; the
                // history discipline is runtime behavior, not graph topology.
                StmtIr::Navigate { route, .. } | StmtIr::NavigateReplace { route, .. } => {
                    (EdgeKind::Navigate, page_id(route.as_str()), None)
                }
                StmtIr::NavigateBack => {
                    (EdgeKind::NavigateBack, "dynamic:previous-page".into(), None)
                }
            };
            edges.push(InteractionEdge {
                id: edge_id(prefix, name, handler_index, "stmt", stmt_index),
                kind,
                from: owner.to_string(),
                to,
                event: event.clone(),
                guard: guard.clone(),
                command,
                outcome: None,
                source: source.clone(),
            });
        }
    }
}

fn event_label(event: &EventKeyIr) -> String {
    match event {
        EventKeyIr::Semantic { event } => event.to_string(),
        EventKeyIr::Outcome { command, which } => format!(
            "{}.{}",
            command,
            match which {
                OutcomeKindIr::Ok => "ok",
                OutcomeKindIr::Err => "err",
            }
        ),
    }
}

fn insert_node(
    nodes: &mut BTreeMap<String, InteractionNode>,
    id: String,
    kind: NodeKind,
    label: &str,
    modality: Option<String>,
) {
    nodes.entry(id.clone()).or_insert_with(|| InteractionNode {
        id,
        kind,
        label: label.to_string(),
        modality,
    });
}

fn page_id(name: &str) -> String {
    format!("page:{name}")
}

fn surface_id(name: &str) -> String {
    format!("surface:{name}")
}

fn command_id(name: &str) -> String {
    format!("command:{name}")
}

fn edge_id(prefix: &str, name: &str, handler: usize, part: &str, index: usize) -> String {
    format!("{prefix}.{name}/handler/{handler}/{part}/{index}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_base::Ident;
    use uhura_core::ir::{CatalogPin, ExprIr, HandlerIr, InitValue, RouteIr, RouteSegIr};

    fn ident(s: &str) -> Ident {
        Ident::new(s).unwrap()
    }

    fn empty_def(modality: Option<&str>) -> DefIr {
        DefIr {
            modality: modality.map(str::to_string),
            props: vec![],
            emits: vec![],
            params: vec![],
            state: BTreeMap::from([(ident("open"), InitValue::Bool(false))]),
            events: BTreeMap::new(),
            handlers: vec![],
            root: uhura_core::ir::NodeIr::Element(uhura_core::ir::ElementIr {
                element: ident("view"),
                ord: 0,
                class: None,
                props: vec![],
                events: vec![],
                text: vec![],
                children: vec![],
            }),
        }
    }

    #[test]
    fn projects_typed_edges_without_inventing_dynamic_targets() {
        let mut feed = empty_def(None);
        feed.handlers.push(HandlerIr {
            on: EventKeyIr::Semantic {
                event: ident("comments-requested"),
            },
            params: vec![],
            guard: Some(ExprIr::Bool(true)),
            body: vec![
                StmtIr::OpenSurface {
                    surface: ident("comments-sheet"),
                    args: vec![],
                },
                StmtIr::Send {
                    port: ident("comments"),
                    command: ident("add-comment"),
                    args: vec![],
                    bind: None,
                },
            ],
        });
        let mut sheet = empty_def(Some("sheet"));
        sheet.handlers.push(HandlerIr {
            on: EventKeyIr::Semantic {
                event: ident("dismiss-requested"),
            },
            params: vec![],
            guard: None,
            body: vec![StmtIr::Dismiss],
        });
        let program = ProgramIr {
            protocol: "uhura-ir/0".into(),
            app: ident("demo"),
            entry: ident("feed"),
            catalog: CatalogPin {
                name: ident("base"),
                version: "0".into(),
                hash: "hash".into(),
            },
            ports: BTreeMap::new(),
            projections: BTreeMap::new(),
            element_events: BTreeMap::new(),
            element_props: BTreeMap::new(),
            routes: BTreeMap::from([(
                ident("feed"),
                RouteIr {
                    segments: vec![RouteSegIr::Static("feed".into())],
                    params: vec![],
                },
            )]),
            pages: BTreeMap::from([(ident("feed"), feed)]),
            components: BTreeMap::new(),
            surfaces: BTreeMap::from([(ident("comments-sheet"), sheet)]),
        };

        let graph = build_interaction_graph(&program);
        assert_eq!(graph.protocol, INTERACTION_GRAPH_PROTOCOL);
        assert!(graph.nodes.iter().any(|n| n.id == "surface:comments-sheet"));
        assert!(graph.edges.iter().any(|e| {
            e.kind == EdgeKind::Present
                && e.from == "page:feed"
                && e.to == "surface:comments-sheet"
                && e.guard.is_some()
        }));
        assert!(graph.edges.iter().any(|e| {
            e.kind == EdgeKind::Dismiss
                && e.from == "surface:comments-sheet"
                && e.to == "dynamic:opener"
        }));
        assert!(graph.edges.iter().any(|e| {
            e.kind == EdgeKind::SendCommand && e.to == "command:comments.add-comment"
        }));
        let a = serde_json::to_value(&graph).unwrap();
        let b = serde_json::to_value(build_interaction_graph(&program)).unwrap();
        assert_eq!(
            uhura_base::to_canonical_json(&a),
            uhura_base::to_canonical_json(&b)
        );
    }
}
