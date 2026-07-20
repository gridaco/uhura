//! Renderer-neutral interaction topology derived from checked Uhura IR.
//!
//! The semantic graph is deliberately independent from physical source
//! layout. Editors receive source navigation as a separate provenance
//! projection produced by the same traversal, so paths and byte offsets can
//! never become part of graph or machine identity.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::ir::{
    Expr, Machine, OutcomePolicy, Program, SourceRef, Statement, UiAttributeValue, UiNode,
};

pub const INTERACTION_GRAPH_PROTOCOL: &str = "uhura-interaction-graph/0";
pub const INTERACTION_GRAPH_PROVENANCE_PROTOCOL: &str = "uhura-interaction-graph-provenance/0";

/// Renderer-neutral, source-layout-independent graph over checked Uhura IR.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionGraph {
    pub protocol: String,
    pub identity_protocol: String,
    pub machine_program_hashes: BTreeMap<String, String>,
    pub presentation_hashes: BTreeMap<String, String>,
    /// Closed policy table keyed by every `Outcome` node ID.
    ///
    /// Keeping policy beside the canonical typed graph makes it inspectable
    /// without encoding it into labels or publishing a second artifact.
    pub outcome_policies: BTreeMap<String, OutcomePolicy>,
    pub nodes: Vec<InteractionGraphNode>,
    pub edges: Vec<InteractionGraphEdge>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct InteractionGraphNode {
    pub id: String,
    pub kind: InteractionGraphNodeKind,
    pub machine: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InteractionGraphNodeKind {
    Module,
    Machine,
    Part,
    Port,
    Input,
    Transition,
    CommitHook,
    State,
    Computed,
    Invariant,
    Update,
    Observation,
    Command,
    Outcome,
    Presentation,
    UiEvent,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct InteractionGraphEdge {
    pub from: String,
    pub to: String,
    pub kind: InteractionGraphEdgeKind,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InteractionGraphEdgeKind {
    Owns,
    Composes,
    Reads,
    Calls,
    Observes,
    Delivers,
    Writes,
    Emits,
    Finishes,
    Triggers,
    Delegates,
    SendsVia,
    Projects,
    Exposes,
    Dispatches,
}

/// Physical source navigation for one semantic interaction graph.
///
/// This artifact is intentionally not embedded in [`InteractionGraph`].
/// A formatter, file move, or byte-offset change may alter this projection
/// without changing the semantic graph.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionGraphProvenance {
    pub protocol: String,
    pub nodes: Vec<InteractionGraphNodeProvenance>,
    pub edges: Vec<InteractionGraphEdgeProvenance>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionGraphNodeProvenance {
    pub node: String,
    pub sources: Vec<SourceRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionGraphEdgeProvenance {
    pub edge: InteractionGraphEdge,
    pub sources: Vec<SourceRef>,
}

/// Both read models produced by one traversal of checked Uhura IR.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionGraphArtifacts {
    pub graph: InteractionGraph,
    pub provenance: InteractionGraphProvenance,
}

/// Builds the source-independent semantic graph.
///
/// Hosts that also need navigation should call
/// [`build_interaction_graph_artifacts`] once and publish both results.
pub fn build_interaction_graph(program: &Program) -> InteractionGraph {
    build_interaction_graph_artifacts(program).graph
}

/// Builds the semantic graph and its separate physical-source projection.
pub fn build_interaction_graph_artifacts(program: &Program) -> InteractionGraphArtifacts {
    let mut builder = InteractionGraphBuilder::default();
    for (machine_id, machine) in &program.machines {
        builder.machine(machine_id, machine);
    }
    for (presentation_id, presentation) in &program.presentations {
        builder.presentation(
            presentation_id,
            &presentation.machine,
            &presentation.nodes,
            &presentation.source,
        );
    }
    builder.finish(program)
}

#[derive(Default)]
struct InteractionGraphBuilder {
    nodes: BTreeMap<String, InteractionGraphNode>,
    edges: BTreeSet<InteractionGraphEdge>,
    node_sources: BTreeMap<String, Vec<SourceRef>>,
    edge_sources: BTreeMap<InteractionGraphEdge, Vec<SourceRef>>,
}

impl InteractionGraphBuilder {
    fn finish(self, program: &Program) -> InteractionGraphArtifacts {
        let outcome_policies = program
            .machines
            .iter()
            .flat_map(|(machine_id, machine)| {
                machine.outcomes.iter().map(move |outcome| {
                    (
                        interaction_node_id("outcome", machine_id, &outcome.constructor.name),
                        outcome.policy,
                    )
                })
            })
            .collect();
        let graph = InteractionGraph {
            protocol: INTERACTION_GRAPH_PROTOCOL.into(),
            identity_protocol: program.identity_protocol.clone(),
            machine_program_hashes: program.program_hashes.clone(),
            presentation_hashes: program.presentation_hashes.clone(),
            outcome_policies,
            nodes: self.nodes.into_values().collect(),
            edges: self.edges.into_iter().collect(),
        };
        let provenance = InteractionGraphProvenance {
            protocol: INTERACTION_GRAPH_PROVENANCE_PROTOCOL.into(),
            nodes: self
                .node_sources
                .into_iter()
                .map(|(node, sources)| InteractionGraphNodeProvenance { node, sources })
                .collect(),
            edges: self
                .edge_sources
                .into_iter()
                .map(|(edge, sources)| InteractionGraphEdgeProvenance { edge, sources })
                .collect(),
        };
        InteractionGraphArtifacts { graph, provenance }
    }

    fn machine(&mut self, machine_id: &str, machine: &Machine) {
        let machine_node = interaction_node_id("machine", machine_id, "");
        self.node(
            machine_node.clone(),
            InteractionGraphNodeKind::Machine,
            machine_id,
            &machine.id,
            &machine.source,
        );

        for port in &machine.ports {
            let port_node = interaction_node_id("port", machine_id, &port.name);
            self.node(
                port_node.clone(),
                InteractionGraphNodeKind::Port,
                machine_id,
                &port.name,
                &port.source,
            );
            self.edge(
                &machine_node,
                &port_node,
                InteractionGraphEdgeKind::Owns,
                &port.source,
            );
            for command in &port.send {
                let type_id = format!("{}::port.{}.Send", machine.id, port.name);
                let constructor = format!("{}.{}", port.name, command.name);
                let command_node = command_node_id(machine_id, &type_id, &constructor);
                self.node(
                    command_node.clone(),
                    InteractionGraphNodeKind::Command,
                    machine_id,
                    &constructor,
                    &port.source,
                );
                self.edge(
                    &machine_node,
                    &command_node,
                    InteractionGraphEdgeKind::Owns,
                    &port.source,
                );
                self.edge(
                    &command_node,
                    &port_node,
                    InteractionGraphEdgeKind::SendsVia,
                    &port.source,
                );
            }
        }

        for state in &machine.state {
            let state_node = interaction_node_id("state", machine_id, &state.name);
            self.node(
                state_node.clone(),
                InteractionGraphNodeKind::State,
                machine_id,
                &state.name,
                &state.source,
            );
            self.edge(
                &machine_node,
                &state_node,
                InteractionGraphEdgeKind::Owns,
                &state.source,
            );
        }
        for outcome in &machine.outcomes {
            let name = &outcome.constructor.name;
            let outcome_node = interaction_node_id("outcome", machine_id, name);
            self.node(
                outcome_node.clone(),
                InteractionGraphNodeKind::Outcome,
                machine_id,
                name,
                &outcome.source,
            );
            self.edge(
                &machine_node,
                &outcome_node,
                InteractionGraphEdgeKind::Owns,
                &outcome.source,
            );
        }
        for command in &machine.local_commands {
            let type_id = format!("{}::Command", machine.id);
            let command_node = command_node_id(machine_id, &type_id, &command.constructor.name);
            self.node(
                command_node.clone(),
                InteractionGraphNodeKind::Command,
                machine_id,
                &command.constructor.name,
                &command.source,
            );
            self.edge(
                &machine_node,
                &command_node,
                InteractionGraphEdgeKind::Owns,
                &command.source,
            );
        }

        for (input, handler) in &machine.handlers {
            let input_node = interaction_node_id("input", machine_id, input);
            self.node(
                input_node.clone(),
                InteractionGraphNodeKind::Input,
                machine_id,
                input,
                &handler.source,
            );
            self.edge(
                &machine_node,
                &input_node,
                InteractionGraphEdgeKind::Owns,
                &handler.source,
            );
            if let Some(port) = machine.ports.iter().find(|port| {
                input
                    .strip_prefix(&format!("{}.", port.name))
                    .is_some_and(|suffix| !suffix.is_empty() && !suffix.contains('.'))
            }) {
                let port_node = interaction_node_id("port", machine_id, &port.name);
                if self.nodes.contains_key(&port_node) {
                    self.edge(
                        &port_node,
                        &input_node,
                        InteractionGraphEdgeKind::Delivers,
                        &handler.source,
                    );
                }
            }
            self.statements(machine, &input_node, &handler.body);
        }
        for (transition_id, transition) in &machine.transitions {
            let transition_node = interaction_node_id("transition", machine_id, transition_id);
            self.node(
                transition_node.clone(),
                InteractionGraphNodeKind::Transition,
                machine_id,
                &transition.name,
                &transition.source,
            );
            self.edge(
                &machine_node,
                &transition_node,
                InteractionGraphEdgeKind::Owns,
                &transition.source,
            );
            self.statements(machine, &transition_node, &transition.body);
        }
        if !machine.before_commit.is_empty() {
            let hook_node = interaction_node_id("commit-hook", machine_id, "before-commit");
            let first_source = statement_source(&machine.before_commit[0]);
            self.node(
                hook_node.clone(),
                InteractionGraphNodeKind::CommitHook,
                machine_id,
                "before commit",
                first_source,
            );
            for statement in &machine.before_commit[1..] {
                self.source_for_node(&hook_node, statement_source(statement));
            }
            self.edge(
                &machine_node,
                &hook_node,
                InteractionGraphEdgeKind::Owns,
                first_source,
            );
            for outcome in &machine.outcomes {
                if outcome.policy == OutcomePolicy::Commit {
                    self.edge(
                        &interaction_node_id("outcome", machine_id, &outcome.constructor.name),
                        &hook_node,
                        InteractionGraphEdgeKind::Triggers,
                        first_source,
                    );
                }
            }
            self.statements(machine, &hook_node, &machine.before_commit);
        }
    }

    fn presentation(
        &mut self,
        presentation_id: &str,
        machine_id: &str,
        nodes: &[UiNode],
        source: &SourceRef,
    ) {
        let presentation_node = interaction_node_id("presentation", machine_id, presentation_id);
        self.node(
            presentation_node.clone(),
            InteractionGraphNodeKind::Presentation,
            machine_id,
            presentation_id,
            source,
        );
        self.edge(
            &presentation_node,
            &interaction_node_id("machine", machine_id, ""),
            InteractionGraphEdgeKind::Projects,
            source,
        );
        let mut event_index = 0usize;
        self.ui_nodes(
            machine_id,
            presentation_id,
            &presentation_node,
            nodes,
            &mut event_index,
        );
    }

    fn ui_nodes(
        &mut self,
        machine_id: &str,
        presentation_id: &str,
        presentation_node: &str,
        nodes: &[UiNode],
        event_index: &mut usize,
    ) {
        for node in nodes {
            match node {
                UiNode::Text { .. } | UiNode::Interpolation { .. } => {}
                UiNode::Element {
                    name,
                    attributes,
                    children,
                    ..
                } => {
                    for attribute in attributes {
                        let UiAttributeValue::Event { event, input } = &attribute.value else {
                            continue;
                        };
                        let event_node = interaction_node_id(
                            "ui-event",
                            machine_id,
                            &format!("{presentation_id}:{event_index:04}"),
                        );
                        *event_index += 1;
                        self.node(
                            event_node.clone(),
                            InteractionGraphNodeKind::UiEvent,
                            machine_id,
                            &format!("{name}.{event}"),
                            &attribute.source,
                        );
                        self.edge(
                            presentation_node,
                            &event_node,
                            InteractionGraphEdgeKind::Exposes,
                            &attribute.source,
                        );
                        for (_, constructor) in root_constructors(input) {
                            let input_node = interaction_node_id("input", machine_id, constructor);
                            if self.nodes.contains_key(&input_node) {
                                self.edge(
                                    &event_node,
                                    &input_node,
                                    InteractionGraphEdgeKind::Dispatches,
                                    &attribute.source,
                                );
                            }
                        }
                    }
                    self.ui_nodes(
                        machine_id,
                        presentation_id,
                        presentation_node,
                        children,
                        event_index,
                    );
                }
                UiNode::If { children, .. } | UiNode::Each { children, .. } => self.ui_nodes(
                    machine_id,
                    presentation_id,
                    presentation_node,
                    children,
                    event_index,
                ),
                UiNode::Match { cases, .. } => {
                    for case in cases {
                        self.ui_nodes(
                            machine_id,
                            presentation_id,
                            presentation_node,
                            &case.children,
                            event_index,
                        );
                    }
                }
            }
        }
    }

    fn statements(&mut self, machine: &Machine, actor: &str, statements: &[Statement]) {
        for statement in statements {
            match statement {
                Statement::Let { .. } | Statement::Unreachable { .. } => {}
                Statement::Set { field, source, .. } => {
                    let state_node = interaction_node_id("state", &machine.id, field);
                    self.source_for_node(&state_node, source);
                    self.edge(actor, &state_node, InteractionGraphEdgeKind::Writes, source);
                }
                Statement::Emit { value, source } => {
                    for (type_id, constructor) in root_constructors(value) {
                        let command_node = command_node_id(&machine.id, type_id, constructor);
                        self.node(
                            command_node.clone(),
                            InteractionGraphNodeKind::Command,
                            &machine.id,
                            constructor,
                            source,
                        );
                        self.edge(
                            actor,
                            &command_node,
                            InteractionGraphEdgeKind::Emits,
                            source,
                        );
                        if let Some(port) = emitted_port(type_id, constructor) {
                            let port_node = interaction_node_id("port", &machine.id, port);
                            if self.nodes.contains_key(&port_node) {
                                self.edge(
                                    &command_node,
                                    &port_node,
                                    InteractionGraphEdgeKind::SendsVia,
                                    source,
                                );
                            }
                        }
                    }
                }
                Statement::Finish { outcome, source } => {
                    for (_, constructor) in root_constructors(outcome) {
                        let outcome_node = interaction_node_id("outcome", &machine.id, constructor);
                        self.source_for_node(&outcome_node, source);
                        self.edge(
                            actor,
                            &outcome_node,
                            InteractionGraphEdgeKind::Finishes,
                            source,
                        );
                    }
                }
                Statement::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.statements(machine, actor, then_body);
                    self.statements(machine, actor, else_body);
                }
                Statement::Match { arms, .. } => {
                    for arm in arms {
                        self.statements(machine, actor, &arm.body);
                    }
                }
                Statement::While { body, .. } => self.statements(machine, actor, body),
                Statement::Delegate {
                    transition, source, ..
                } => {
                    let transition_id = machine
                        .transitions
                        .keys()
                        .find(|identity| {
                            identity.as_str() == transition
                                || identity
                                    .rsplit_once("::")
                                    .is_some_and(|(_, name)| name == transition)
                        })
                        .map_or(transition.as_str(), String::as_str);
                    let transition_node =
                        interaction_node_id("transition", &machine.id, transition_id);
                    self.source_for_node(&transition_node, source);
                    self.edge(
                        actor,
                        &transition_node,
                        InteractionGraphEdgeKind::Delegates,
                        source,
                    );
                }
            }
        }
    }

    fn node(
        &mut self,
        id: String,
        kind: InteractionGraphNodeKind,
        machine: &str,
        label: &str,
        source: &SourceRef,
    ) {
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| InteractionGraphNode {
                id: id.clone(),
                kind,
                machine: machine.into(),
                label: label.into(),
            });
        self.source_for_node(&id, source);
    }

    fn edge(&mut self, from: &str, to: &str, kind: InteractionGraphEdgeKind, source: &SourceRef) {
        let edge = InteractionGraphEdge {
            from: from.into(),
            to: to.into(),
            kind,
        };
        self.edges.insert(edge.clone());
        push_source(self.edge_sources.entry(edge).or_default(), source);
    }

    fn source_for_node(&mut self, node: &str, source: &SourceRef) {
        push_source(self.node_sources.entry(node.into()).or_default(), source);
    }
}

fn statement_source(statement: &Statement) -> &SourceRef {
    match statement {
        Statement::Let { source, .. }
        | Statement::Set { source, .. }
        | Statement::Emit { source, .. }
        | Statement::If { source, .. }
        | Statement::Match { source, .. }
        | Statement::While { source, .. }
        | Statement::Finish { source, .. }
        | Statement::Unreachable { source }
        | Statement::Delegate { source, .. } => source,
    }
}

fn push_source(sources: &mut Vec<SourceRef>, source: &SourceRef) {
    if !sources.contains(source) {
        sources.push(source.clone());
        sources.sort_by(compare_source);
    }
}

fn compare_source(left: &SourceRef, right: &SourceRef) -> Ordering {
    (&left.path, left.start, left.end, &left.id).cmp(&(
        &right.path,
        right.start,
        right.end,
        &right.id,
    ))
}

/// Stable, renderer-neutral identity used by the canonical interaction graph.
///
/// Checker-owned source topology uses this same constructor before source
/// composition is erased from runtime IR.
#[must_use]
pub fn interaction_node_id(kind: &str, machine: &str, member: &str) -> String {
    if member.is_empty() {
        format!("{kind}:{machine}")
    } else {
        format!("{kind}:{machine}:{member}")
    }
}

fn command_node_id(machine: &str, type_id: &str, constructor: &str) -> String {
    interaction_node_id("command", machine, &format!("{type_id}:{constructor}"))
}

fn emitted_port<'a>(type_id: &'a str, constructor: &'a str) -> Option<&'a str> {
    let marker = "::port.";
    let suffix = type_id.split_once(marker)?.1;
    let port = suffix.strip_suffix(".Send")?;
    constructor
        .strip_prefix(port)
        .and_then(|rest| rest.starts_with('.').then_some(port))
}

/// Constructors an expression can produce at its root. Nested payload
/// constructors are data, not dispatch, command, or outcome identities.
fn root_constructors(expression: &Expr) -> Vec<(&str, &str)> {
    match expression {
        Expr::Constructor {
            type_id,
            constructor,
            ..
        } => vec![(type_id, constructor)],
        Expr::If {
            then_value,
            else_value,
            ..
        } => {
            let mut constructors = root_constructors(then_value);
            constructors.extend(root_constructors(else_value));
            constructors
        }
        Expr::Match { arms, .. } => arms
            .iter()
            .flat_map(|arm| root_constructors(&arm.value))
            .collect(),
        Expr::Let { value, .. } => root_constructors(value),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StateField;
    use crate::value::{IntegerKind, Value};
    use crate::{
        ConstructorDef, Handler, OutcomeDef, Pattern, Presentation, TypeDef, TypeRef, UiAttribute,
    };

    fn source(id: &str, start: u32) -> SourceRef {
        SourceRef {
            id: id.into(),
            path: "app.uhura".into(),
            start,
            end: start + 3,
        }
    }

    fn program() -> Program {
        let mut program = Program::new();
        let machine_id = "example@1::Counter".to_string();
        let input_type = format!("{machine_id}::Input");
        let outcome_type = format!("{machine_id}::Outcome");
        let literal = |value: i64| Expr::Literal {
            value: Value::Integer {
                kind: IntegerKind::Int,
                value: value.into(),
            },
        };
        program.machines.insert(
            machine_id.clone(),
            Machine {
                id: machine_id.clone(),
                config: TypeRef::Unit,
                requires: vec![],
                ports: vec![],
                local_input: TypeDef::Sum {
                    id: input_type.clone(),
                    constructors: vec![ConstructorDef {
                        name: "increment".into(),
                        fields: vec![],
                    }],
                },
                local_commands: vec![],
                outcomes: vec![OutcomeDef {
                    constructor: ConstructorDef {
                        name: "accepted".into(),
                        fields: vec![],
                    },
                    policy: OutcomePolicy::Commit,
                    source: source("outcome", 20),
                }],
                state: vec![StateField {
                    name: "count".into(),
                    ty: TypeRef::Int,
                    initial: literal(0),
                    source: source("count", 30),
                }],
                functions: BTreeMap::new(),
                derives: vec![],
                invariants: vec![],
                observation: vec![],
                transitions: BTreeMap::new(),
                handlers: BTreeMap::from([(
                    "increment".into(),
                    Handler {
                        input: "increment".into(),
                        pattern: Pattern::Constructor {
                            type_id: input_type.clone(),
                            constructor: "increment".into(),
                            fields: vec![],
                        },
                        body: vec![
                            Statement::Set {
                                field: "count".into(),
                                value: literal(1),
                                source: source("set", 50),
                            },
                            Statement::Finish {
                                outcome: Expr::Constructor {
                                    type_id: outcome_type,
                                    constructor: "accepted".into(),
                                    fields: vec![],
                                },
                                source: source("finish", 60),
                            },
                        ],
                        source: source("handler", 40),
                    },
                )]),
                before_commit: vec![],
                source: source("machine", 10),
            },
        );
        program.presentations.insert(
            "example@1::CounterView".into(),
            Presentation {
                id: "example@1::CounterView".into(),
                machine: machine_id,
                binding: "observation".into(),
                nodes: vec![UiNode::Element {
                    name: "button".into(),
                    attributes: vec![UiAttribute {
                        name: "on".into(),
                        value: UiAttributeValue::Event {
                            event: "press".into(),
                            input: Expr::Constructor {
                                type_id: input_type,
                                constructor: "increment".into(),
                                fields: vec![],
                            },
                        },
                        source: source("event", 80),
                    }],
                    children: vec![],
                    source: source("button", 70),
                }],
                source: source("presentation", 65),
            },
        );
        program.freeze_program_hashes();
        program
    }

    #[test]
    fn semantic_graph_and_physical_provenance_are_separate_and_closed() {
        let mut program = program();
        let first = build_interaction_graph_artifacts(&program);
        assert_eq!(first.graph.protocol, INTERACTION_GRAPH_PROTOCOL);
        assert_eq!(
            first.provenance.protocol,
            INTERACTION_GRAPH_PROVENANCE_PROTOCOL
        );
        assert_eq!(
            first.graph.outcome_policies,
            BTreeMap::from([(
                "outcome:example@1::Counter:accepted".into(),
                OutcomePolicy::Commit,
            )]),
        );
        let nodes = first
            .graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(first.graph.edges.iter().all(|edge| {
            nodes.contains(edge.from.as_str()) && nodes.contains(edge.to.as_str())
        }));
        assert!(
            first
                .provenance
                .nodes
                .iter()
                .all(|entry| { nodes.contains(entry.node.as_str()) && !entry.sources.is_empty() })
        );
        assert!(
            first
                .provenance
                .edges
                .iter()
                .all(|entry| first.graph.edges.contains(&entry.edge) && !entry.sources.is_empty())
        );

        program
            .machines
            .get_mut("example@1::Counter")
            .expect("machine")
            .source
            .path = "moved/app.uhura".into();
        let second = build_interaction_graph_artifacts(&program);
        assert_eq!(first.graph, second.graph);
        assert_ne!(first.provenance, second.provenance);
    }

    #[test]
    fn action_sources_are_attached_to_semantic_targets_and_edges() {
        let artifacts = build_interaction_graph_artifacts(&program());
        let state = artifacts
            .provenance
            .nodes
            .iter()
            .find(|entry| entry.node.ends_with(":count"))
            .expect("state provenance");
        assert_eq!(
            state
                .sources
                .iter()
                .map(|source| source.id.as_str())
                .collect::<Vec<_>>(),
            vec!["count", "set"]
        );
        let dispatch = artifacts
            .provenance
            .edges
            .iter()
            .find(|entry| entry.edge.kind == InteractionGraphEdgeKind::Dispatches)
            .expect("dispatch provenance");
        assert_eq!(dispatch.sources[0].id, "event");
    }
}
