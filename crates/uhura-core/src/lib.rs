//! Uhura's machine-first semantic core.
//!
//! The core is pure and I/O-free. Checked source lowers to [`ir::Program`],
//! and [`runtime::Instance`] is the single reference execution model used by
//! native and Wasm consumers.
#![deny(clippy::float_arithmetic)]

pub mod codec;
pub mod evidence;
pub mod graph;
pub mod ir;
pub mod provenance;
pub mod render;
pub mod route;
pub mod runtime;
pub mod typed;
pub mod value;

pub use evidence::{
    EVIDENCE_REPORT_PROTOCOL, EvidenceArtifacts, EvidenceCheckpointArtifact,
    EvidenceExampleArtifact, EvidenceFailure, EvidenceFailureCode, EvidencePinArtifact,
    EvidenceReport, EvidenceRunner, EvidenceSnapshot, FixtureSnapshot, ScenarioReport,
    ScenarioStatus,
};
pub use graph::{
    INTERACTION_GRAPH_PROTOCOL, INTERACTION_GRAPH_PROVENANCE_PROTOCOL, InteractionGraph,
    InteractionGraphArtifacts, InteractionGraphEdge, InteractionGraphEdgeKind,
    InteractionGraphEdgeProvenance, InteractionGraphNode, InteractionGraphNodeKind,
    InteractionGraphNodeProvenance, InteractionGraphProvenance, build_interaction_graph,
    build_interaction_graph_artifacts, interaction_node_id,
};
pub use ir::{
    BinaryOp, CommandDef, ConstructorDef, DEPLOYMENT_ID_PROTOCOL, DeploymentContentIdentity,
    DeploymentIdentityMaterial, DeploymentPortBinding, DeploymentPresentationIdentity,
    EvidenceExampleMetadata, EvidencePresentationKind, EvidenceRef, EvidenceStep, EvidenceSuite,
    Expr, Function, Handler, IR_PROTOCOL, MACHINE_PROGRAM_ID_PROTOCOL,
    MACHINE_UI_INTERFACE_ID_PROTOCOL, Machine, MatchArm, OutcomeDef, OutcomePolicy,
    PRESENTATION_ID_PROTOCOL, Pattern, Presentation, Program, SEMANTIC_IR_IDENTITY_PROTOCOL,
    Scenario, ScenarioOrigin, SiteIdentityFrame, Statement, TypeDef, TypeRef, UiAttribute,
    UiAttributeValue, UiCase, UiNode, UnaryOp, deployment_hash, deployment_hash_v04,
    deployment_identity_bytes, deployment_identity_bytes_v04,
};
pub use provenance::{
    AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL, AuthoredInteractionEdge, AuthoredInteractionNode,
    AuthoredInteractionTopology, NODE_ID_PROTOCOL, PROVENANCE_PROTOCOL, Provenance,
    ProvenanceOccurrence, ProvenanceSelector, ProvenanceSource, SOURCE_REVISION_ID_PROTOCOL,
    merge_authored_interaction_topology, semantic_node_id, source_revision_id,
};
pub use render::{
    EventBinding, PROJECTION_SOURCES_PROTOCOL, Projection, ProjectionSources, RenderAttribute,
    RenderAttributeValue, RenderDocument, RenderError, RenderEvent, RenderNode, VIEW_PROTOCOL,
};
pub use route::RouteRuntimeError;
pub use runtime::{
    AdmissionError, CHECKPOINT_PROTOCOL, Checkpoint, GENESIS_RECEIPT_PROTOCOL, GenesisReceipt,
    INGRESS_RECORD_PROTOCOL, INLINE_UPDATE_JOIN_LOCAL_PREFIX, INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX,
    IngressAttempt, IngressError, IngressRecord, IngressRejectionKind, Instance, InstanceLifecycle,
    PURE_CONTINUATION_LOCAL_PREFIX, ProgramFault, REACTION_RECEIPT_PROTOCOL, ReactionReceipt,
    ReactionResolution, RestoreError, RuntimeError, Step,
};
pub use typed::{ValueTypeError, canonical_type_identity_bytes};
pub use value::{BoundaryNumber, Decimal, DecimalError, IntegerKind, Value, ValueError};
