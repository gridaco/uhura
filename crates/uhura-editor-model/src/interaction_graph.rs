//! Application-oriented interaction graph carried by the Editor read model.
//!
//! This graph is intentionally a presentation model, not an interpreter. The
//! canonical machine graph remains `uhura_core::InteractionGraph` and is
//! published independently in the machine sidecar.

use serde::Serialize;

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
    fn default() -> Self {
        Self {
            protocol: INTERACTION_GRAPH_PROTOCOL.into(),
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
