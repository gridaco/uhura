//! Versioned, browser-neutral Editor read model.
//!
//! This crate serializes already-checked Uhura machine projections. It does
//! not parse source, lower programs, evaluate templates, or implement a second
//! runtime. Hosts construct previews from `uhura_core::Program` evidence and
//! `uhura_core::Projection`; the Editor receives one immutable read model.

pub mod interaction_graph;

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{SourceMap, Span, Value, to_canonical_json};
use uhura_core::{
    MACHINE_PROGRAM_ID_PROTOCOL, PROJECTION_SOURCES_PROTOCOL, Projection, Provenance, RenderNode,
    VIEW_PROTOCOL,
};

pub use uhura_core::{ProjectionSources, RenderDocument};

pub const EDITOR_STATE_PROTOCOL: &str = "uhura-editor-state/4";
pub const MACHINE_SIDECAR_PROTOCOL: &str = "uhura-machine-inspection/0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorState {
    pub protocol: String,
    pub source_revision: u64,
    /// A complete `uhura-diagnostics/0` envelope, or JSON `null`.
    pub diagnostics: serde_json::Value,
    /// `None` is the intentional cold-invalid representation.
    pub render: Option<EditorRender>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorRender {
    pub revision: u64,
    pub freshness: RenderFreshness,
    pub application: Application,
    pub authoring: AuthoringMetadata,
    pub groups: Vec<PreviewGroup>,
    pub previews: Vec<Preview>,
    pub stylesheet: String,
    pub assets: BTreeMap<String, Asset>,
    /// Application-oriented workflow graph used by the Editor canvas.
    pub interaction_graph: interaction_graph::InteractionGraph,
    /// Canonical machine inspection artifacts, independent from the
    /// application-oriented graph above.
    pub machine: Option<MachineSidecar>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineSidecar {
    pub protocol: String,
    pub identity_protocol: String,
    pub deployment: Option<MachineDeployment>,
    pub sources: serde_json::Value,
    /// Source-layout-sensitive semantic occurrences for the checked project.
    pub provenance: Provenance,
    pub interaction_graph: serde_json::Value,
    pub graph_sources: serde_json::Value,
    pub checkpoints: serde_json::Value,
    pub evidence: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineSidecarInput {
    pub identity_protocol: String,
    pub deployment: Option<MachineDeployment>,
    pub sources: serde_json::Value,
    pub provenance: Provenance,
    pub interaction_graph: serde_json::Value,
    pub graph_sources: serde_json::Value,
    pub checkpoints: serde_json::Value,
    pub evidence: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineDeployment {
    pub entry: String,
    pub machine: String,
    pub presentation: Option<String>,
    pub instance: String,
    pub machine_program_hash: String,
    pub presentation_hash: Option<String>,
    pub evidence_hash: Option<String>,
    pub deployment_hash: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFreshness {
    Current,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Application {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewGroup {
    pub id: String,
    pub kind: PreviewKind,
    pub subject: String,
    pub previews: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub id: String,
    pub identity: PreviewIdentity,
    pub source_file: String,
    pub is_default: bool,
    pub pinned: bool,
    pub derived: bool,
    pub in_flight: usize,
    pub from: Option<String>,
    pub replay_steps: Vec<String>,
    pub replay: Vec<serde_json::Value>,
    pub note: Option<String>,
    pub data: Vec<PreviewField>,
    pub interactions: Vec<Interaction>,
    pub documentation: PreviewDocumentation,
    pub provenance: PreviewProvenance,
    pub evidence: Option<PreviewEvidence>,
    pub content: PreviewContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewEvidence {
    pub scenario: String,
    pub pin: String,
    pub source_id: String,
    pub registration_source: serde_json::Value,
    pub pin_source: serde_json::Value,
    pub observation: serde_json::Value,
    pub snapshot: serde_json::Value,
    pub scenario_receipt_log: Option<serde_json::Value>,
}

/// A 1-based source location, matching `uhura-diagnostics/0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EditorSourcePosition {
    pub line: u32,
    pub col: u32,
}

/// A diagnostics-shaped half-open byte span within a separately named file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorSourceSpan {
    pub offset: u32,
    pub len: u32,
    pub start: EditorSourcePosition,
    pub end: EditorSourcePosition,
}

impl EditorSourceSpan {
    /// Converts a compiler span without leaking `FileId` onto the wire.
    pub fn from_span(source_map: &SourceMap, span: Span) -> Self {
        let start = source_map.line_col(span.file, span.start);
        let end = source_map.line_col(span.file, span.end);
        Self {
            offset: span.start,
            len: span.len(),
            start: EditorSourcePosition {
                line: start.line,
                col: start.col,
            },
            end: EditorSourcePosition {
                line: end.line,
                col: end.col,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceTargetClass {
    SourceModule,
    ComponentDeclaration,
    PageDeclaration,
    SurfaceDeclaration,
    PropDeclaration,
    EmittedEventDeclaration,
    EmittedEventParameter,
    RouteParameter,
    StoreScope,
    StateField,
    EventHandler,
    OutcomeHandler,
    HandlerParameter,
    ExampleDeclaration,
    UiElement,
    ComponentInvocation,
    IfBlock,
    EachBlock,
    MatchBlock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceTargetOwnerKind {
    Module,
    Examples,
    Component,
    Page,
    Surface,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceTargetOwner {
    pub kind: SourceTargetOwnerKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceTarget {
    pub id: String,
    pub class: SourceTargetClass,
    pub file: String,
    pub span: EditorSourceSpan,
    pub label: String,
    pub owner: SourceTargetOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceMetadataClass {
    Doc,
    Annotation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMetadataEntry {
    pub id: String,
    pub class: SourceMetadataClass,
    pub kind: String,
    pub text: String,
    pub span: EditorSourceSpan,
    pub target_id: String,
    pub order: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthoringMetadata {
    pub targets: Vec<SourceTarget>,
    pub entries: Vec<SourceMetadataEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewDocumentation {
    pub declaration_doc_id: Option<String>,
    pub example_doc_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetOccurrence {
    pub id: String,
    pub target_id: String,
    /// Opaque semantic node keys from the preview's `uhura-view/1` document.
    pub anchors: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewProvenance {
    pub occurrences: Vec<TargetOccurrence>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PreviewIdentity {
    pub kind: PreviewKind,
    pub subject: String,
    pub example: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreviewKind {
    Page,
    Surface,
    Component,
}

/// The only preview payload accepted by the Editor wire contract.
///
/// The wire discriminant remains explicit, but the type system prevents a
/// host from publishing a second, Editor-only view model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewContent {
    Projection(ProjectionContent),
}

/// Static machine projection plus its exact source-navigation sidecar.
///
/// Event bindings are intentionally excluded: Editor previews are inert, and
/// Play reconstructs live bindings from the admitted machine instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionContent {
    pub document: RenderDocument,
    pub sources: ProjectionSources,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewField {
    pub group: PreviewFieldGroup,
    pub name: String,
    pub key: Option<Value>,
    pub value: PreviewFieldValue,
    pub source: Option<PreviewFieldSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewFieldGroup {
    Properties,
    PageAddress,
    ProvidedData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewFieldValue {
    Ready(Value),
    /// Exact tagged machine JSON.
    ReadyJson(serde_json::Value),
    Waiting,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewFieldSource {
    pub kind: PreviewFieldSourceKind,
    pub declared_in: Option<String>,
    pub timeline: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewFieldSourceKind {
    Inline,
    Fixture { fixture: String, path: Vec<String> },
    AutomaticFixture { fixture: String, path: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interaction {
    pub node_key: String,
    pub element: String,
    pub kind: InteractionKind,
    pub event: String,
    pub emit: String,
    pub scope: String,
    pub payload: serde_json::Value,
    pub carries: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionKind {
    Input,
    Observe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub data_uri: String,
    pub alt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    UnsupportedProtocol(String),
    InvalidDiagnosticsEnvelope,
    RevisionMustBePositive { field: &'static str },
    CurrentRevisionMismatch { source: u64, render: u64 },
    StaleRevisionNotOlder { source: u64, render: u64 },
    Invalid(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedProtocol(protocol) => write!(
                formatter,
                "Editor state protocol `{protocol}` is not supported (this build reads `{EDITOR_STATE_PROTOCOL}`)"
            ),
            Self::InvalidDiagnosticsEnvelope => formatter
                .write_str("Editor state diagnostics must be `uhura-diagnostics/0` or null"),
            Self::RevisionMustBePositive { field } => {
                write!(formatter, "Editor state `{field}` must be at least 1")
            }
            Self::CurrentRevisionMismatch { source, render } => write!(
                formatter,
                "current render revision {render} must equal source revision {source}"
            ),
            Self::StaleRevisionNotOlder { source, render } => write!(
                formatter,
                "stale render revision {render} must be older than source revision {source}"
            ),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ValidationError {}

impl EditorState {
    pub fn current(
        source_revision: u64,
        diagnostics: serde_json::Value,
        mut render: EditorRender,
    ) -> Result<Self, ValidationError> {
        render.freshness = RenderFreshness::Current;
        let state = Self {
            protocol: EDITOR_STATE_PROTOCOL.into(),
            source_revision,
            diagnostics,
            render: Some(render),
        };
        state.validate()?;
        Ok(state)
    }

    pub fn stale(
        source_revision: u64,
        diagnostics: serde_json::Value,
        mut last_renderable: EditorRender,
    ) -> Result<Self, ValidationError> {
        last_renderable.freshness = RenderFreshness::Stale;
        let state = Self {
            protocol: EDITOR_STATE_PROTOCOL.into(),
            source_revision,
            diagnostics,
            render: Some(last_renderable),
        };
        state.validate()?;
        Ok(state)
    }

    pub fn cold_invalid(
        source_revision: u64,
        diagnostics: serde_json::Value,
    ) -> Result<Self, ValidationError> {
        let state = Self {
            protocol: EDITOR_STATE_PROTOCOL.into(),
            source_revision,
            diagnostics,
            render: None,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.protocol != EDITOR_STATE_PROTOCOL {
            return Err(ValidationError::UnsupportedProtocol(self.protocol.clone()));
        }
        if self.source_revision == 0 {
            return Err(ValidationError::RevisionMustBePositive {
                field: "sourceRevision",
            });
        }
        if !valid_diagnostics_envelope(&self.diagnostics) {
            return Err(ValidationError::InvalidDiagnosticsEnvelope);
        }
        let Some(render) = &self.render else {
            return Ok(());
        };
        if render.revision == 0 {
            return Err(ValidationError::RevisionMustBePositive {
                field: "render.revision",
            });
        }
        match render.freshness {
            RenderFreshness::Current if render.revision != self.source_revision => {
                return Err(ValidationError::CurrentRevisionMismatch {
                    source: self.source_revision,
                    render: render.revision,
                });
            }
            RenderFreshness::Stale if render.revision >= self.source_revision => {
                return Err(ValidationError::StaleRevisionNotOlder {
                    source: self.source_revision,
                    render: render.revision,
                });
            }
            RenderFreshness::Current | RenderFreshness::Stale => {}
        }
        render.validate()
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "protocol": self.protocol,
            "sourceRevision": self.source_revision,
            "diagnostics": self.diagnostics,
            "render": self.render.as_ref().map(EditorRender::to_json),
        })
    }

    pub fn to_canonical_string(&self) -> Result<String, ValidationError> {
        self.validate()?;
        Ok(to_canonical_json(&self.to_json()))
    }
}

impl EditorRender {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.application.name.is_empty() {
            return invalid("Editor application name must not be empty");
        }
        if self.interaction_graph.protocol != interaction_graph::INTERACTION_GRAPH_PROTOCOL {
            return invalid("Editor interaction graph protocol is invalid");
        }
        if let Some(machine) = &self.machine {
            machine.validate()?;
        }

        let authoring = self.authoring.validate()?;
        let mut preview_ids = BTreeSet::new();
        let mut identities = BTreeSet::new();
        let mut previews = BTreeMap::new();
        for preview in &self.previews {
            if !preview_ids.insert(preview.id.as_str()) {
                return invalid(format!("preview id `{}` occurs more than once", preview.id));
            }
            if !identities.insert(&preview.identity) {
                return invalid(format!(
                    "preview identity `{}/{}/{}` occurs more than once",
                    preview.identity.kind.as_str(),
                    preview.identity.subject,
                    preview.identity.example
                ));
            }
            let expected = stable_preview_id(&preview.identity);
            if preview.id != expected {
                return invalid(format!(
                    "preview id `{}` does not match stable id `{expected}`",
                    preview.id
                ));
            }
            preview.validate(&authoring.targets, &authoring.entries)?;
            previews.insert(preview.id.as_str(), preview);
        }

        for preview in &self.previews {
            let Some(parent) = &preview.from else {
                continue;
            };
            let parent_id = stable_preview_id(&PreviewIdentity {
                kind: preview.identity.kind,
                subject: preview.identity.subject.clone(),
                example: parent.clone(),
            });
            if !previews.contains_key(parent_id.as_str()) {
                return invalid(format!(
                    "preview `{}` refers to unknown parent example `{parent}`",
                    preview.id
                ));
            }
        }

        let mut group_ids = BTreeSet::new();
        let mut grouped = BTreeSet::new();
        for group in &self.groups {
            if !group_ids.insert(group.id.as_str()) {
                return invalid(format!(
                    "preview group id `{}` occurs more than once",
                    group.id
                ));
            }
            if group.id != stable_group_id(group.kind, &group.subject) {
                return invalid(format!("preview group `{}` has an unstable id", group.id));
            }
            for preview_id in &group.previews {
                let Some(preview) = previews.get(preview_id.as_str()) else {
                    return invalid(format!(
                        "group `{}` refers to unknown preview `{preview_id}`",
                        group.id
                    ));
                };
                if !grouped.insert(preview_id.as_str()) {
                    return invalid(format!(
                        "preview `{preview_id}` belongs to more than one group"
                    ));
                }
                if preview.identity.kind != group.kind || preview.identity.subject != group.subject
                {
                    return invalid(format!(
                        "preview `{preview_id}` does not match group `{}`",
                        group.id
                    ));
                }
            }
        }
        if let Some(preview) = preview_ids.difference(&grouped).next() {
            return invalid(format!("preview `{preview}` does not belong to a group"));
        }
        Ok(())
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "revision": self.revision,
            "freshness": self.freshness.as_str(),
            "application": { "name": self.application.name },
            "authoring": self.authoring.to_json(),
            "groups": self.groups.iter().map(PreviewGroup::to_json).collect::<Vec<_>>(),
            "previews": self.previews.iter().map(Preview::to_json).collect::<Vec<_>>(),
            "stylesheet": self.stylesheet,
            "assets": self.assets.iter().map(|(id, asset)| {
                (id.clone(), asset.to_json())
            }).collect::<serde_json::Map<_, _>>(),
            "interactionGraph": serde_json::to_value(&self.interaction_graph)
                .expect("Editor interaction graphs serialize"),
            "machine": self.machine.as_ref().map(MachineSidecar::to_json),
        })
    }
}

impl MachineSidecar {
    pub fn new(input: MachineSidecarInput) -> Self {
        Self {
            protocol: MACHINE_SIDECAR_PROTOCOL.into(),
            identity_protocol: input.identity_protocol,
            deployment: input.deployment,
            sources: input.sources,
            provenance: input.provenance,
            interaction_graph: input.interaction_graph,
            graph_sources: input.graph_sources,
            checkpoints: input.checkpoints,
            evidence: input.evidence,
        }
    }

    fn validate(&self) -> Result<(), ValidationError> {
        if self.protocol != MACHINE_SIDECAR_PROTOCOL {
            return invalid(format!(
                "machine sidecar protocol must be `{MACHINE_SIDECAR_PROTOCOL}`"
            ));
        }
        if self.identity_protocol != MACHINE_PROGRAM_ID_PROTOCOL {
            return invalid(format!(
                "machine sidecar identity protocol must be `{MACHINE_PROGRAM_ID_PROTOCOL}`"
            ));
        }
        if !self.sources.is_array() {
            return invalid("machine sidecar sources must be an array");
        }
        self.provenance.validate().map_err(|error| {
            ValidationError::Invalid(format!("invalid machine provenance: {error}"))
        })?;
        for (field, value) in [
            ("interactionGraph", &self.interaction_graph),
            ("graphSources", &self.graph_sources),
            ("checkpoints", &self.checkpoints),
            ("evidence", &self.evidence),
        ] {
            if !value.is_object() {
                return invalid(format!("machine sidecar {field} must be an object"));
            }
        }
        if let Some(deployment) = &self.deployment {
            for (field, value) in [
                ("entry", deployment.entry.as_str()),
                ("machine", deployment.machine.as_str()),
                ("instance", deployment.instance.as_str()),
                (
                    "machineProgramHash",
                    deployment.machine_program_hash.as_str(),
                ),
                ("deploymentHash", deployment.deployment_hash.as_str()),
            ] {
                if value.is_empty() {
                    return invalid(format!(
                        "machine sidecar deployment.{field} must not be empty"
                    ));
                }
            }
            if [
                deployment.presentation.as_deref(),
                deployment.presentation_hash.as_deref(),
                deployment.evidence_hash.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(str::is_empty)
            {
                return invalid(
                    "machine sidecar optional deployment fields must be absent instead of empty",
                );
            }
        }
        Ok(())
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "protocol": self.protocol,
            "identityProtocol": self.identity_protocol,
            "deployment": self.deployment.as_ref().map(MachineDeployment::to_json),
            "sources": self.sources,
            "provenance": self.provenance,
            "interactionGraph": self.interaction_graph,
            "graphSources": self.graph_sources,
            "checkpoints": self.checkpoints,
            "evidence": self.evidence,
        })
    }
}

impl ProjectionContent {
    pub fn from_projection(projection: Projection) -> Self {
        Self {
            document: projection.document,
            sources: projection.sources,
        }
    }

    fn validate(&self, preview: &str) -> Result<(), ValidationError> {
        if self.document.protocol != VIEW_PROTOCOL {
            return invalid(format!(
                "preview `{preview}` projection document protocol must be `{VIEW_PROTOCOL}`"
            ));
        }
        if self.sources.protocol != PROJECTION_SOURCES_PROTOCOL {
            return invalid(format!(
                "preview `{preview}` projection source protocol must be `{PROJECTION_SOURCES_PROTOCOL}`"
            ));
        }
        if self.document.presentation != self.sources.presentation {
            return invalid(format!(
                "preview `{preview}` projection document and source presentations differ"
            ));
        }
        let mut rendered = BTreeSet::new();
        collect_projection_keys(&self.document.nodes, &mut rendered).map_err(|message| {
            ValidationError::Invalid(format!("preview `{preview}` projection: {message}"))
        })?;
        let sourced = self.sources.nodes.keys().cloned().collect::<BTreeSet<_>>();
        if rendered != sourced {
            return invalid(format!(
                "preview `{preview}` projection sources must cover every rendered node exactly once"
            ));
        }
        Ok(())
    }
}

impl From<Projection> for ProjectionContent {
    fn from(projection: Projection) -> Self {
        Self::from_projection(projection)
    }
}

impl From<Projection> for PreviewContent {
    fn from(projection: Projection) -> Self {
        Self::Projection(projection.into())
    }
}

impl Preview {
    fn validate(
        &self,
        targets: &BTreeMap<&str, &SourceTarget>,
        entries: &BTreeMap<&str, &SourceMetadataEntry>,
    ) -> Result<(), ValidationError> {
        if !canonical_source_path(&self.source_file) {
            return invalid(format!("preview `{}` has an invalid source file", self.id));
        }
        let PreviewContent::Projection(projection) = &self.content;
        projection.validate(&self.id)?;
        if let Some(evidence) = &self.evidence {
            evidence.validate(&self.id)?;
        }
        if self.replay.len() != self.replay_steps.len()
            || self
                .replay
                .iter()
                .zip(&self.replay_steps)
                .any(|(step, label)| {
                    step.get("label").and_then(serde_json::Value::as_str) != Some(label.as_str())
                })
        {
            return invalid(format!(
                "preview `{}` replay details do not match replay step labels",
                self.id
            ));
        }
        for entry in [
            self.documentation.declaration_doc_id.as_deref(),
            self.documentation.example_doc_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !entries.contains_key(entry) {
                return invalid(format!(
                    "preview `{}` refers to unknown documentation entry `{entry}`",
                    self.id
                ));
            }
        }
        let mut occurrences = BTreeSet::new();
        for occurrence in &self.provenance.occurrences {
            if occurrence.id.is_empty() || occurrence.target_id.is_empty() {
                return invalid(format!(
                    "preview `{}` has an incomplete source occurrence",
                    self.id
                ));
            }
            if !occurrences.insert(occurrence.id.as_str()) {
                return invalid(format!(
                    "preview `{}` repeats occurrence `{}`",
                    self.id, occurrence.id
                ));
            }
            if !targets.contains_key(occurrence.target_id.as_str()) {
                return invalid(format!(
                    "preview `{}` occurrence `{}` refers to unknown target `{}`",
                    self.id, occurrence.id, occurrence.target_id
                ));
            }
            let mut anchors = BTreeSet::new();
            for anchor in &occurrence.anchors {
                if !anchors.insert(anchor) {
                    return invalid(format!(
                        "preview `{}` occurrence `{}` repeats an anchor",
                        self.id, occurrence.id
                    ));
                }
                let resolves = !anchor.is_empty()
                    && projection_contains_key(&projection.document.nodes, anchor);
                if !resolves {
                    return invalid(format!(
                        "preview `{}` occurrence `{}` has an invalid anchor",
                        self.id, occurrence.id
                    ));
                }
            }
        }
        Ok(())
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "identity": self.identity.to_json(),
            "sourceFile": self.source_file,
            "default": self.is_default,
            "pinned": self.pinned,
            "derived": self.derived,
            "inFlight": self.in_flight,
            "from": self.from,
            "replaySteps": self.replay_steps,
            "replay": self.replay,
            "note": self.note,
            "data": self.data.iter().map(PreviewField::to_json).collect::<Vec<_>>(),
            "interactions": self.interactions.iter().map(Interaction::to_json).collect::<Vec<_>>(),
            "documentation": self.documentation.to_json(),
            "provenance": self.provenance.to_json(),
            "evidence": self.evidence.as_ref().map(PreviewEvidence::to_json),
            "content": self.content.to_json(),
        })
    }
}

impl PreviewEvidence {
    fn validate(&self, preview: &str) -> Result<(), ValidationError> {
        if self.scenario.is_empty() || self.pin.is_empty() || self.source_id.is_empty() {
            return invalid(format!(
                "preview `{preview}` has incomplete evidence identity"
            ));
        }
        if !self.registration_source.is_object()
            || !self.pin_source.is_object()
            || !self.snapshot.is_object()
            || self.observation.is_null()
        {
            return invalid(format!(
                "preview `{preview}` has malformed evidence payloads"
            ));
        }
        Ok(())
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "scenario": self.scenario,
            "pin": self.pin,
            "sourceId": self.source_id,
            "sources": {
                "registration": self.registration_source,
                "pin": self.pin_source,
            },
            "observation": self.observation,
            "snapshot": self.snapshot,
            "scenarioReceiptLog": self.scenario_receipt_log,
        })
    }
}

struct AuthoringIndex<'a> {
    targets: BTreeMap<&'a str, &'a SourceTarget>,
    entries: BTreeMap<&'a str, &'a SourceMetadataEntry>,
}

impl AuthoringMetadata {
    fn validate(&self) -> Result<AuthoringIndex<'_>, ValidationError> {
        let mut targets = BTreeMap::new();
        for target in &self.targets {
            if target.id.is_empty()
                || target.label.is_empty()
                || target.owner.name.is_empty()
                || !canonical_source_path(&target.file)
                || !valid_source_span(target.span)
            {
                return invalid("Editor authoring contains an invalid source target");
            }
            if targets.insert(target.id.as_str(), target).is_some() {
                return invalid(format!(
                    "source target id `{}` occurs more than once",
                    target.id
                ));
            }
        }
        let mut entries = BTreeMap::new();
        let mut orders = BTreeMap::<&str, Vec<usize>>::new();
        for entry in &self.entries {
            if entry.id.is_empty()
                || entry.kind.is_empty()
                || entry.text.is_empty()
                || !valid_source_span(entry.span)
                || entry.span.len == 0
            {
                return invalid("Editor authoring contains an invalid metadata entry");
            }
            if entries.insert(entry.id.as_str(), entry).is_some() {
                return invalid(format!(
                    "source metadata entry id `{}` occurs more than once",
                    entry.id
                ));
            }
            if !targets.contains_key(entry.target_id.as_str()) {
                return invalid(format!(
                    "source metadata entry `{}` refers to unknown target `{}`",
                    entry.id, entry.target_id
                ));
            }
            orders
                .entry(entry.target_id.as_str())
                .or_default()
                .push(entry.order);
        }
        for (target, order) in &mut orders {
            order.sort_unstable();
            if order
                .iter()
                .copied()
                .enumerate()
                .any(|(index, value)| value != index)
            {
                return invalid(format!(
                    "source target `{target}` metadata order must be contiguous"
                ));
            }
        }
        Ok(AuthoringIndex { targets, entries })
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "targets": self.targets.iter().map(SourceTarget::to_json).collect::<Vec<_>>(),
            "entries": self.entries.iter().map(SourceMetadataEntry::to_json).collect::<Vec<_>>(),
        })
    }
}

impl PreviewContent {
    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Projection(projection) => {
                let mut document = serde_json::to_value(&projection.document)
                    .expect("checked Uhura projections serialize");
                document["sequence"] =
                    serde_json::Value::String(projection.document.sequence.to_string());
                serde_json::json!({
                    "kind": "projection",
                    "value": {
                        "document": document,
                        "sources": projection.sources,
                    },
                })
            }
        }
    }
}

impl MachineDeployment {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "entry": self.entry,
            "machine": self.machine,
            "presentation": self.presentation,
            "instance": self.instance,
            "machineProgramHash": self.machine_program_hash,
            "presentationHash": self.presentation_hash,
            "evidenceHash": self.evidence_hash,
            "deploymentHash": self.deployment_hash,
        })
    }
}

impl SourceTarget {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "class": self.class.as_str(),
            "file": self.file,
            "span": self.span.to_json(),
            "label": self.label,
            "owner": self.owner.to_json(),
        })
    }
}

impl SourceMetadataEntry {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "class": self.class.as_str(),
            "kind": self.kind,
            "text": self.text,
            "span": self.span.to_json(),
            "targetId": self.target_id,
            "order": self.order,
        })
    }
}

impl EditorSourceSpan {
    fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "offset": self.offset,
            "len": self.len,
            "start": { "line": self.start.line, "col": self.start.col },
            "end": { "line": self.end.line, "col": self.end.col },
        })
    }
}

impl SourceTargetOwner {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind.as_str(),
            "name": self.name,
        })
    }
}

impl PreviewGroup {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "kind": self.kind.as_str(),
            "subject": self.subject,
            "previews": self.previews,
        })
    }
}

impl PreviewDocumentation {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "declarationDocId": self.declaration_doc_id,
            "exampleDocId": self.example_doc_id,
        })
    }
}

impl PreviewProvenance {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "occurrences": self.occurrences.iter().map(TargetOccurrence::to_json).collect::<Vec<_>>(),
        })
    }
}

impl TargetOccurrence {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "targetId": self.target_id,
            "anchors": self.anchors,
        })
    }
}

impl PreviewIdentity {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind.as_str(),
            "subject": self.subject,
            "example": self.example,
        })
    }
}

impl PreviewField {
    fn to_json(&self) -> serde_json::Value {
        let mut json = serde_json::json!({
            "group": self.group.as_str(),
            "name": self.name,
            "key": self.key.as_ref().map(Value::to_json),
            "status": self.value.status(),
            "source": self.source.as_ref().map(PreviewFieldSource::to_json),
        });
        match &self.value {
            PreviewFieldValue::Ready(value) => json["value"] = value.to_json(),
            PreviewFieldValue::ReadyJson(value) => json["value"] = value.clone(),
            PreviewFieldValue::Waiting => {}
            PreviewFieldValue::Failed(reason) => {
                json["reason"] = serde_json::Value::String(reason.clone());
            }
        }
        json
    }
}

impl PreviewFieldSource {
    fn to_json(&self) -> serde_json::Value {
        let mut json = serde_json::json!({
            "kind": self.kind.as_str(),
            "declaredIn": self.declared_in,
            "timeline": self.timeline,
        });
        match &self.kind {
            PreviewFieldSourceKind::Inline => {}
            PreviewFieldSourceKind::Fixture { fixture, path }
            | PreviewFieldSourceKind::AutomaticFixture { fixture, path } => {
                json["fixture"] = serde_json::Value::String(fixture.clone());
                json["path"] = serde_json::json!(path);
            }
        }
        json
    }
}

impl Interaction {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "nodeKey": self.node_key,
            "element": self.element,
            "kind": self.kind.as_str(),
            "event": self.event,
            "emit": self.emit,
            "scope": self.scope,
            "payload": self.payload,
            "carries": self.carries,
        })
    }
}

impl Asset {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "dataUri": self.data_uri,
            "alt": self.alt,
        })
    }
}

impl RenderFreshness {
    fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
        }
    }
}

impl SourceTargetClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::SourceModule => "source-module",
            Self::ComponentDeclaration => "component-declaration",
            Self::PageDeclaration => "page-declaration",
            Self::SurfaceDeclaration => "surface-declaration",
            Self::PropDeclaration => "prop-declaration",
            Self::EmittedEventDeclaration => "emitted-event-declaration",
            Self::EmittedEventParameter => "emitted-event-parameter",
            Self::RouteParameter => "route-parameter",
            Self::StoreScope => "store-scope",
            Self::StateField => "state-field",
            Self::EventHandler => "event-handler",
            Self::OutcomeHandler => "outcome-handler",
            Self::HandlerParameter => "handler-parameter",
            Self::ExampleDeclaration => "example-declaration",
            Self::UiElement => "ui-element",
            Self::ComponentInvocation => "component-invocation",
            Self::IfBlock => "if-block",
            Self::EachBlock => "each-block",
            Self::MatchBlock => "match-block",
        }
    }
}

impl SourceTargetOwnerKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Examples => "examples",
            Self::Component => "component",
            Self::Page => "page",
            Self::Surface => "surface",
        }
    }
}

impl SourceMetadataClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Doc => "doc",
            Self::Annotation => "annotation",
        }
    }
}

impl PreviewKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Surface => "surface",
            Self::Component => "component",
        }
    }
}

impl PreviewFieldGroup {
    fn as_str(self) -> &'static str {
        match self {
            Self::Properties => "properties",
            Self::PageAddress => "page-address",
            Self::ProvidedData => "provided-data",
        }
    }
}

impl PreviewFieldValue {
    fn status(&self) -> &'static str {
        match self {
            Self::Ready(_) | Self::ReadyJson(_) => "ready",
            Self::Waiting => "waiting",
            Self::Failed(_) => "failed",
        }
    }
}

impl PreviewFieldSourceKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Fixture { .. } => "fixture",
            Self::AutomaticFixture { .. } => "automatic-fixture",
        }
    }
}

impl InteractionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Observe => "observe",
        }
    }
}

pub fn stable_preview_id(identity: &PreviewIdentity) -> String {
    format!(
        "{}/{}/{}",
        identity.kind.as_str(),
        identity.subject,
        identity.example
    )
}

pub fn stable_group_id(kind: PreviewKind, subject: &str) -> String {
    format!("{}/{subject}", kind.as_str())
}

fn collect_projection_keys(
    nodes: &[RenderNode],
    keys: &mut BTreeSet<String>,
) -> Result<(), String> {
    for node in nodes {
        let (key, children) = match node {
            RenderNode::Text { key, .. } => (key, None),
            RenderNode::Element { key, children, .. } => (key, Some(children.as_slice())),
        };
        if key.is_empty() {
            return Err("rendered-node keys must not be empty".into());
        }
        if !keys.insert(key.clone()) {
            return Err(format!("rendered-node key `{key}` occurs more than once"));
        }
        if let Some(children) = children {
            collect_projection_keys(children, keys)?;
        }
    }
    Ok(())
}

fn projection_contains_key(nodes: &[RenderNode], expected: &str) -> bool {
    nodes.iter().any(|node| match node {
        RenderNode::Text { key, .. } => key == expected,
        RenderNode::Element { key, children, .. } => {
            key == expected || projection_contains_key(children, expected)
        }
    })
}

fn valid_diagnostics_envelope(value: &serde_json::Value) -> bool {
    if value.is_null() {
        return true;
    }
    let Some(envelope) = value.as_object() else {
        return false;
    };
    if envelope.get("format").and_then(serde_json::Value::as_str) != Some("uhura-diagnostics")
        || envelope.get("version").and_then(serde_json::Value::as_u64) != Some(0)
    {
        return false;
    }
    let Some(summary) = envelope
        .get("summary")
        .and_then(serde_json::Value::as_object)
    else {
        return false;
    };
    let (Some(expected_errors), Some(expected_warnings)) = (
        summary.get("errors").and_then(serde_json::Value::as_u64),
        summary.get("warnings").and_then(serde_json::Value::as_u64),
    ) else {
        return false;
    };
    let Some(diagnostics) = envelope
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
    else {
        return false;
    };
    let mut errors = 0;
    let mut warnings = 0;
    for diagnostic in diagnostics {
        let Some(diagnostic) = diagnostic.as_object() else {
            return false;
        };
        if !["code", "rule", "severity", "message"]
            .into_iter()
            .all(|key| {
                diagnostic
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .is_some()
            })
        {
            return false;
        }
        match diagnostic
            .get("severity")
            .and_then(serde_json::Value::as_str)
        {
            Some("error") => errors += 1,
            Some("warning") => warnings += 1,
            Some("info") => {}
            _ => return false,
        }
    }
    errors == expected_errors && warnings == expected_warnings
}

fn canonical_source_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn valid_source_span(span: EditorSourceSpan) -> bool {
    span.offset.checked_add(span.len).is_some()
        && span.start.line > 0
        && span.start.col > 0
        && span.end.line > 0
        && span.end.col > 0
        && span.start <= span.end
        && ((span.len == 0) == (span.start == span.end))
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError::Invalid(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_core::ir::SourceRef;
    use uhura_core::{ProjectionSources, RenderDocument};

    fn projection() -> ProjectionContent {
        ProjectionContent {
            document: RenderDocument {
                protocol: VIEW_PROTOCOL.into(),
                presentation: "app.web@1::main".into(),
                machine: "app.machine@1::App".into(),
                instance: "editor/example".into(),
                sequence: 9_007_199_254_740_993,
                nodes: vec![RenderNode::Text {
                    key: "text:1".into(),
                    text: "Hello".into(),
                }],
            },
            sources: ProjectionSources {
                protocol: PROJECTION_SOURCES_PROTOCOL.into(),
                presentation: "app.web@1::main".into(),
                nodes: BTreeMap::from([("text:1".into(), SourceRef::synthetic("source:text"))]),
            },
        }
    }

    fn render(revision: u64) -> EditorRender {
        let identity = PreviewIdentity {
            kind: PreviewKind::Page,
            subject: "app.web@1::main".into(),
            example: "canonical".into(),
        };
        let id = stable_preview_id(&identity);
        EditorRender {
            revision,
            freshness: RenderFreshness::Current,
            application: Application { name: "app".into() },
            authoring: AuthoringMetadata::default(),
            groups: vec![PreviewGroup {
                id: stable_group_id(identity.kind, &identity.subject),
                kind: identity.kind,
                subject: identity.subject.clone(),
                previews: vec![id.clone()],
            }],
            previews: vec![Preview {
                id,
                identity,
                source_file: "app.uhura".into(),
                is_default: true,
                pinned: true,
                derived: false,
                in_flight: 0,
                from: None,
                replay_steps: Vec::new(),
                replay: Vec::new(),
                note: None,
                data: Vec::new(),
                interactions: Vec::new(),
                documentation: PreviewDocumentation::default(),
                provenance: PreviewProvenance::default(),
                evidence: None,
                content: PreviewContent::Projection(projection()),
            }],
            stylesheet: String::new(),
            assets: BTreeMap::new(),
            interaction_graph: interaction_graph::InteractionGraph {
                protocol: interaction_graph::INTERACTION_GRAPH_PROTOCOL.into(),
                app: "app".into(),
                entry: "page:app.web@1::main".into(),
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            machine: None,
        }
    }

    #[test]
    fn projection_state_is_canonical_and_keeps_exact_sequence_text() {
        let state = EditorState::current(1, serde_json::Value::Null, render(1)).unwrap();
        let json = state.to_json();
        assert_eq!(
            json["render"]["previews"][0]["content"]["value"]["document"]["protocol"],
            VIEW_PROTOCOL
        );
        assert_eq!(
            json["render"]["previews"][0]["content"]["value"]["document"]["sequence"],
            "9007199254740993"
        );
        assert!(state.to_canonical_string().unwrap().starts_with('{'));
    }

    #[test]
    fn stale_and_cold_states_enforce_revision_order() {
        assert!(EditorState::stale(2, serde_json::Value::Null, render(1)).is_ok());
        assert!(EditorState::stale(1, serde_json::Value::Null, render(1)).is_err());
        assert!(EditorState::cold_invalid(1, serde_json::Value::Null).is_ok());
    }

    #[test]
    fn projection_sources_must_cover_the_document_exactly() {
        let mut content = projection();
        content.sources.nodes.clear();
        let error = content.validate("preview").unwrap_err().to_string();
        assert!(error.contains("cover every rendered node"));
    }

    #[test]
    fn machine_sidecar_admits_only_the_current_identity_protocol() {
        let sidecar = |identity: &str| {
            MachineSidecar::new(MachineSidecarInput {
                identity_protocol: identity.into(),
                deployment: None,
                sources: serde_json::json!([]),
                provenance: Provenance::canonical(Vec::new(), Vec::new()).unwrap(),
                interaction_graph: serde_json::json!({}),
                graph_sources: serde_json::json!({}),
                checkpoints: serde_json::json!({}),
                evidence: serde_json::json!({}),
            })
        };
        assert!(sidecar(MACHINE_PROGRAM_ID_PROTOCOL).validate().is_ok());
        assert!(sidecar("uhura-unrecognized-identity/9").validate().is_err());
        let mut invalid_provenance = sidecar(MACHINE_PROGRAM_ID_PROTOCOL);
        invalid_provenance.provenance.protocol = "uhura-provenance/retired".into();
        assert!(invalid_provenance.validate().is_err());
    }
}
