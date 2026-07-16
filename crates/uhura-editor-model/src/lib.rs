//! Versioned, browser-neutral Editor read model.
//!
//! This crate is the boundary between checked Uhura projects and the web
//! application. It evaluates every resolved example into semantic
//! `uhura-core` view data and serializes one immutable, versioned state. It
//! deliberately owns no HTML, CSS, DOM identifiers, browser event handling,
//! concrete icon geometry, filesystem access, or transport.

pub mod interaction_graph;

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{SourceMap, Span, Value, has_errors, hash_json, to_canonical_json, to_envelope};
use uhura_check::CheckOutput;
use uhura_check::metadata::{
    AuthoringProjection as CheckedAuthoringProjection, MetadataClass as CheckedMetadataClass,
    SourceOwnerKind as CheckedOwnerKind, SourceTargetClass as CheckedTargetClass,
};
use uhura_check::preview::{
    PreviewDataKind, PreviewDataValue, PreviewOrigin, PreviewPayload,
    PreviewSource as CheckedPreviewSource, ResolvedPreview,
};
use uhura_check::resolve::SubjectKind;
use uhura_core::eval::{eval_fragment_with_trace, eval_view_with_trace};
use uhura_core::template::{
    DefinitionAddress, DefinitionKind, EvaluationContext, EvaluationContextSegment,
    EvaluationTrace, TemplateAddress, TemplateSegment,
};
pub use uhura_core::template::{RenderNodeRef, RenderRoot};
use uhura_core::view::{Descriptor, DescriptorKind, Node, Snapshot};

pub const EDITOR_STATE_PROTOCOL: &str = "uhura-editor-state/2";

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
    /// The app's static interaction structure (`uhura-interaction-graph/0`),
    /// projected from the same checked program as the previews.
    pub interaction_graph: interaction_graph::InteractionGraph,
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
    pub content: PreviewContent,
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
    CatalogElement,
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
    pub anchors: Vec<RenderNodeRef>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewContent {
    Page(Snapshot),
    Fragment(Node),
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

/// One renderer-neutral summary of a semantic descriptor in the preview.
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

/// One browser-consumable asset. The native host decides how bytes become a
/// URI; this model only carries the immutable table for a state revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub data_uri: String,
    pub alt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    DirtyCheck,
    MissingProgram,
    MissingDefinition {
        preview: String,
        definition: String,
    },
    MissingTemplateOrigin {
        preview: String,
        template: TemplateAddress,
    },
    Evaluation {
        preview: String,
        message: String,
    },
    InvalidAuthoring(String),
    InvalidState(ValidationError),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::DirtyCheck => write!(f, "Editor model requires a clean check output"),
            BuildError::MissingProgram => write!(f, "clean check output has no lowered program"),
            BuildError::MissingDefinition {
                preview,
                definition,
            } => write!(
                f,
                "preview `{preview}` refers to missing definition `{definition}`"
            ),
            BuildError::MissingTemplateOrigin { preview, template } => write!(
                f,
                "could not build preview `{preview}` provenance: no source origin for template {}",
                to_canonical_json(&template_address_json(template))
            ),
            BuildError::Evaluation { preview, message } => {
                write!(f, "could not evaluate preview `{preview}`: {message}")
            }
            BuildError::InvalidAuthoring(message) => {
                write!(f, "could not build Editor authoring metadata: {message}")
            }
            BuildError::InvalidState(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for BuildError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    UnsupportedProtocol(String),
    InvalidDiagnosticsEnvelope,
    RevisionMustBePositive {
        field: &'static str,
    },
    CurrentRevisionMismatch {
        source: u64,
        render: u64,
    },
    StaleRevisionNotOlder {
        source: u64,
        render: u64,
    },
    DuplicatePreviewId(String),
    DuplicatePreviewIdentity(PreviewIdentity),
    InvalidPreviewId {
        expected: String,
        actual: String,
    },
    UnknownPreviewParent {
        preview: String,
        parent: String,
    },
    ReplayMetadataMismatch(String),
    DuplicateGroupId(String),
    UnknownGroupedPreview {
        group: String,
        preview: String,
    },
    PreviewGroupedMoreThanOnce(String),
    UngroupedPreview(String),
    GroupIdentityMismatch {
        group: String,
        preview: String,
    },
    ContentKindMismatch(String),
    DuplicateSourceTargetId(String),
    DuplicateMetadataEntryId(String),
    InvalidAuthoringField {
        owner: String,
        field: &'static str,
    },
    UnknownMetadataTarget {
        entry: String,
        target: String,
    },
    IncompatibleMetadataTarget {
        entry: String,
        target: String,
    },
    InvalidMetadataOrder {
        target: String,
        expected: usize,
        actual: usize,
    },
    UnknownDocumentationEntry {
        preview: String,
        entry: String,
    },
    IncompatibleDocumentationEntry {
        preview: String,
        entry: String,
    },
    DuplicateOccurrenceId {
        preview: String,
        occurrence: String,
    },
    UnknownOccurrenceTarget {
        preview: String,
        occurrence: String,
        target: String,
    },
    IncompatibleOccurrenceTarget {
        preview: String,
        occurrence: String,
        target: String,
    },
    DuplicateOccurrenceAnchor {
        preview: String,
        occurrence: String,
    },
    InvalidOccurrenceAnchor {
        preview: String,
        occurrence: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::UnsupportedProtocol(protocol) => write!(
                f,
                "Editor state protocol `{protocol}` is not supported (this build reads \
                 `{EDITOR_STATE_PROTOCOL}`)"
            ),
            ValidationError::InvalidDiagnosticsEnvelope => {
                write!(
                    f,
                    "Editor state diagnostics must be `uhura-diagnostics/0` or null"
                )
            }
            ValidationError::RevisionMustBePositive { field } => {
                write!(f, "Editor state `{field}` must be at least 1")
            }
            ValidationError::CurrentRevisionMismatch { source, render } => write!(
                f,
                "current render revision {render} must equal source revision {source}"
            ),
            ValidationError::StaleRevisionNotOlder { source, render } => write!(
                f,
                "stale render revision {render} must be older than source revision {source}"
            ),
            ValidationError::DuplicatePreviewId(id) => {
                write!(f, "preview id `{id}` occurs more than once")
            }
            ValidationError::DuplicatePreviewIdentity(identity) => write!(
                f,
                "preview identity `{} / {} / {}` occurs more than once",
                identity.kind.as_str(),
                identity.subject,
                identity.example
            ),
            ValidationError::InvalidPreviewId { expected, actual } => write!(
                f,
                "preview id `{actual}` does not match its stable identity id `{expected}`"
            ),
            ValidationError::UnknownPreviewParent { preview, parent } => write!(
                f,
                "preview `{preview}` refers to unknown parent example `{parent}`"
            ),
            ValidationError::ReplayMetadataMismatch(preview) => write!(
                f,
                "preview `{preview}` replay details do not match replay step labels"
            ),
            ValidationError::DuplicateGroupId(id) => {
                write!(f, "preview group id `{id}` occurs more than once")
            }
            ValidationError::UnknownGroupedPreview { group, preview } => {
                write!(f, "group `{group}` refers to unknown preview `{preview}`")
            }
            ValidationError::PreviewGroupedMoreThanOnce(preview) => {
                write!(f, "preview `{preview}` belongs to more than one group")
            }
            ValidationError::UngroupedPreview(preview) => {
                write!(f, "preview `{preview}` does not belong to a group")
            }
            ValidationError::GroupIdentityMismatch { group, preview } => write!(
                f,
                "preview `{preview}` does not match group `{group}` kind and subject"
            ),
            ValidationError::ContentKindMismatch(preview) => write!(
                f,
                "preview `{preview}` has content inconsistent with its identity kind"
            ),
            ValidationError::DuplicateSourceTargetId(id) => {
                write!(f, "source target id `{id}` occurs more than once")
            }
            ValidationError::DuplicateMetadataEntryId(id) => {
                write!(f, "source metadata entry id `{id}` occurs more than once")
            }
            ValidationError::InvalidAuthoringField { owner, field } => {
                write!(f, "authoring item `{owner}` has invalid `{field}`")
            }
            ValidationError::UnknownMetadataTarget { entry, target } => write!(
                f,
                "source metadata entry `{entry}` refers to unknown target `{target}`"
            ),
            ValidationError::IncompatibleMetadataTarget { entry, target } => write!(
                f,
                "source metadata entry `{entry}` is incompatible with target `{target}`"
            ),
            ValidationError::InvalidMetadataOrder {
                target,
                expected,
                actual,
            } => write!(
                f,
                "target `{target}` metadata order must be contiguous (expected {expected}, got {actual})"
            ),
            ValidationError::UnknownDocumentationEntry { preview, entry } => write!(
                f,
                "preview `{preview}` refers to unknown documentation entry `{entry}`"
            ),
            ValidationError::IncompatibleDocumentationEntry { preview, entry } => write!(
                f,
                "preview `{preview}` refers to incompatible documentation entry `{entry}`"
            ),
            ValidationError::DuplicateOccurrenceId {
                preview,
                occurrence,
            } => write!(
                f,
                "preview `{preview}` occurrence id `{occurrence}` occurs more than once"
            ),
            ValidationError::UnknownOccurrenceTarget {
                preview,
                occurrence,
                target,
            } => write!(
                f,
                "preview `{preview}` occurrence `{occurrence}` refers to unknown target `{target}`"
            ),
            ValidationError::IncompatibleOccurrenceTarget {
                preview,
                occurrence,
                target,
            } => write!(
                f,
                "preview `{preview}` occurrence `{occurrence}` is incompatible with target `{target}`"
            ),
            ValidationError::DuplicateOccurrenceAnchor {
                preview,
                occurrence,
            } => write!(
                f,
                "preview `{preview}` occurrence `{occurrence}` repeats an anchor"
            ),
            ValidationError::InvalidOccurrenceAnchor {
                preview,
                occurrence,
            } => write!(
                f,
                "preview `{preview}` occurrence `{occurrence}` has an invalid semantic anchor"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Evaluates all previews in a clean check result into one current state.
///
/// Diagnostics are the clean revision's warnings/info, if any. Errors and a
/// missing lowered program are rejected instead of producing a partial model.
pub fn build_current_state(
    source_revision: u64,
    output: &CheckOutput,
    assets: BTreeMap<String, Asset>,
) -> Result<EditorState, BuildError> {
    if source_revision == 0 {
        return Err(BuildError::InvalidState(
            ValidationError::RevisionMustBePositive {
                field: "sourceRevision",
            },
        ));
    }
    let render = build_render(source_revision, output, assets)?;
    let diagnostics = diagnostics_json(output);
    EditorState::current(source_revision, diagnostics, render).map_err(BuildError::InvalidState)
}

/// Adapts the checker's lowered span table to the interaction-graph span
/// lookup without making the projection crate depend on checker internals.
struct LoweredSpans<'a>(&'a BTreeMap<String, uhura_check::lower::SpanEntry>);

impl interaction_graph::SpanLookup for LoweredSpans<'_> {
    fn source_ref(&self, ir_path: &str) -> Option<interaction_graph::SourceRef> {
        self.0
            .get(ir_path)
            .map(|span| interaction_graph::SourceRef {
                file: span.file.clone(),
                start: span.start,
                end: span.end,
                ir_path: ir_path.to_string(),
            })
    }
}

/// Evaluates all previews but does not choose current/stale envelope status.
/// Hosts use this when retaining a last-renderable payload across rejected
/// source revisions.
pub fn build_render(
    revision: u64,
    output: &CheckOutput,
    assets: BTreeMap<String, Asset>,
) -> Result<EditorRender, BuildError> {
    if revision == 0 {
        return Err(BuildError::InvalidState(
            ValidationError::RevisionMustBePositive {
                field: "render.revision",
            },
        ));
    }
    if has_errors(&output.diagnostics) {
        return Err(BuildError::DirtyCheck);
    }
    let lowered = output.lowered.as_ref().ok_or(BuildError::MissingProgram)?;
    let program = &lowered.program;
    let (authoring, annotation_targets) =
        build_authoring_metadata(&output.authoring, &output.source_map)?;

    let mut previews = Vec::with_capacity(output.previews.len());
    for checked in &output.previews {
        previews.push(build_preview(
            program,
            &lowered.template_origins,
            &annotation_targets,
            checked,
        )?);
    }

    let mut groups = Vec::<PreviewGroup>::new();
    let mut group_indexes = BTreeMap::<(PreviewKind, String), usize>::new();
    for preview in &previews {
        let group_key = (preview.identity.kind, preview.identity.subject.clone());
        let index = match group_indexes.get(&group_key) {
            Some(index) => *index,
            None => {
                let index = groups.len();
                groups.push(PreviewGroup {
                    id: stable_group_id(group_key.0, &group_key.1),
                    kind: group_key.0,
                    subject: group_key.1.clone(),
                    previews: Vec::new(),
                });
                group_indexes.insert(group_key, index);
                index
            }
        };
        groups[index].previews.push(preview.id.clone());
    }

    let render = EditorRender {
        revision,
        freshness: RenderFreshness::Current,
        application: Application {
            name: program.app.to_string(),
        },
        authoring,
        groups,
        previews,
        stylesheet: output.stylesheet.clone(),
        assets,
        interaction_graph: interaction_graph::build_interaction_graph_with_spans(
            program,
            &LoweredSpans(&lowered.spans),
        ),
    };
    let state = EditorState {
        protocol: EDITOR_STATE_PROTOCOL.to_string(),
        source_revision: revision,
        diagnostics: serde_json::Value::Null,
        render: Some(render.clone()),
    };
    state.validate().map_err(BuildError::InvalidState)?;
    Ok(render)
}

/// Returns the checker's stable diagnostics envelope, or JSON `null` when
/// there is nothing to report.
pub fn diagnostics_json(output: &CheckOutput) -> serde_json::Value {
    if output.diagnostics.is_empty() {
        serde_json::Value::Null
    } else {
        to_envelope(&output.diagnostics, &output.source_map)
    }
}

impl EditorState {
    pub fn current(
        source_revision: u64,
        diagnostics: serde_json::Value,
        mut render: EditorRender,
    ) -> Result<Self, ValidationError> {
        render.freshness = RenderFreshness::Current;
        let state = Self {
            protocol: EDITOR_STATE_PROTOCOL.to_string(),
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
            protocol: EDITOR_STATE_PROTOCOL.to_string(),
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
            protocol: EDITOR_STATE_PROTOCOL.to_string(),
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
    let mut errors = 0_u64;
    let mut warnings = 0_u64;
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

struct AuthoringIndex<'a> {
    targets: BTreeMap<&'a str, &'a SourceTarget>,
    entries: BTreeMap<&'a str, &'a SourceMetadataEntry>,
    annotation_targets: BTreeSet<&'a str>,
}

impl AuthoringMetadata {
    fn validate(&self) -> Result<AuthoringIndex<'_>, ValidationError> {
        let mut targets = BTreeMap::new();
        for target in &self.targets {
            if target.id.is_empty() {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: "source target".to_string(),
                    field: "id",
                });
            }
            if targets.insert(target.id.as_str(), target).is_some() {
                return Err(ValidationError::DuplicateSourceTargetId(target.id.clone()));
            }
            if !canonical_source_path(&target.file) {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: target.id.clone(),
                    field: "file",
                });
            }
            if !valid_source_span(target.span) {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: target.id.clone(),
                    field: "span",
                });
            }
            if target.label.is_empty() {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: target.id.clone(),
                    field: "label",
                });
            }
            if target.owner.name.is_empty()
                || matches!(
                    target.owner.kind,
                    SourceTargetOwnerKind::Module | SourceTargetOwnerKind::Examples
                ) && target.owner.name != target.file
            {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: target.id.clone(),
                    field: "owner",
                });
            }
        }

        let mut entries = BTreeMap::new();
        let mut orders = BTreeMap::<&str, Vec<usize>>::new();
        let mut annotation_targets = BTreeSet::new();
        for entry in &self.entries {
            if entry.id.is_empty() {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: "source metadata entry".to_string(),
                    field: "id",
                });
            }
            if entries.insert(entry.id.as_str(), entry).is_some() {
                return Err(ValidationError::DuplicateMetadataEntryId(entry.id.clone()));
            }
            if entry.text.is_empty()
                || entry.target_id.is_empty()
                || !valid_source_span(entry.span)
                || entry.span.len == 0
            {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: entry.id.clone(),
                    field: if entry.text.is_empty() {
                        "text"
                    } else if entry.target_id.is_empty() {
                        "targetId"
                    } else {
                        "span"
                    },
                });
            }
            let target = targets.get(entry.target_id.as_str()).ok_or_else(|| {
                ValidationError::UnknownMetadataTarget {
                    entry: entry.id.clone(),
                    target: entry.target_id.clone(),
                }
            })?;
            let compatible = match entry.class {
                SourceMetadataClass::Doc => {
                    entry.kind == "doc" && entry.order == 0 && target.class.documentable()
                }
                SourceMetadataClass::Annotation => {
                    valid_annotation_kind(&entry.kind) && target.class.annotatable()
                }
            };
            if !compatible {
                return Err(ValidationError::IncompatibleMetadataTarget {
                    entry: entry.id.clone(),
                    target: entry.target_id.clone(),
                });
            }
            if entry.class == SourceMetadataClass::Annotation {
                annotation_targets.insert(entry.target_id.as_str());
            }
            orders
                .entry(entry.target_id.as_str())
                .or_default()
                .push(entry.order);
        }
        for (target, values) in &mut orders {
            values.sort_unstable();
            for (expected, actual) in values.iter().copied().enumerate() {
                if actual != expected {
                    return Err(ValidationError::InvalidMetadataOrder {
                        target: (*target).to_string(),
                        expected,
                        actual,
                    });
                }
            }
        }
        if let Some(target) = targets.keys().find(|target| !orders.contains_key(**target)) {
            return Err(ValidationError::InvalidAuthoringField {
                owner: (*target).to_string(),
                field: "entries",
            });
        }
        Ok(AuthoringIndex {
            targets,
            entries,
            annotation_targets,
        })
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "targets": self.targets.iter().map(SourceTarget::to_json).collect::<Vec<_>>(),
            "entries": self.entries.iter().map(SourceMetadataEntry::to_json).collect::<Vec<_>>(),
        })
    }
}

impl Preview {
    fn validate_authoring(&self, authoring: &AuthoringIndex<'_>) -> Result<(), ValidationError> {
        if !canonical_source_path(&self.source_file) {
            return Err(ValidationError::InvalidAuthoringField {
                owner: self.id.clone(),
                field: "sourceFile",
            });
        }
        self.validate_documentation_entry(
            authoring,
            self.documentation.declaration_doc_id.as_deref(),
            match self.identity.kind {
                PreviewKind::Page => SourceTargetClass::PageDeclaration,
                PreviewKind::Surface => SourceTargetClass::SurfaceDeclaration,
                PreviewKind::Component => SourceTargetClass::ComponentDeclaration,
            },
        )?;
        self.validate_documentation_entry(
            authoring,
            self.documentation.example_doc_id.as_deref(),
            SourceTargetClass::ExampleDeclaration,
        )?;

        let mut occurrence_ids = BTreeSet::new();
        for occurrence in &self.provenance.occurrences {
            if occurrence.id.is_empty() || occurrence.target_id.is_empty() {
                return Err(ValidationError::InvalidAuthoringField {
                    owner: format!("preview {} occurrence", self.id),
                    field: if occurrence.id.is_empty() {
                        "id"
                    } else {
                        "targetId"
                    },
                });
            }
            if !occurrence_ids.insert(occurrence.id.as_str()) {
                return Err(ValidationError::DuplicateOccurrenceId {
                    preview: self.id.clone(),
                    occurrence: occurrence.id.clone(),
                });
            }
            let target = authoring
                .targets
                .get(occurrence.target_id.as_str())
                .ok_or_else(|| ValidationError::UnknownOccurrenceTarget {
                    preview: self.id.clone(),
                    occurrence: occurrence.id.clone(),
                    target: occurrence.target_id.clone(),
                })?;
            if !target.class.annotatable()
                || !authoring
                    .annotation_targets
                    .contains(occurrence.target_id.as_str())
            {
                return Err(ValidationError::IncompatibleOccurrenceTarget {
                    preview: self.id.clone(),
                    occurrence: occurrence.id.clone(),
                    target: occurrence.target_id.clone(),
                });
            }
            let mut anchors = BTreeSet::new();
            for anchor in &occurrence.anchors {
                if !anchors.insert(anchor) {
                    return Err(ValidationError::DuplicateOccurrenceAnchor {
                        preview: self.id.clone(),
                        occurrence: occurrence.id.clone(),
                    });
                }
                if !self.anchor_resolves(anchor) {
                    return Err(ValidationError::InvalidOccurrenceAnchor {
                        preview: self.id.clone(),
                        occurrence: occurrence.id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_documentation_entry(
        &self,
        authoring: &AuthoringIndex<'_>,
        id: Option<&str>,
        expected_target: SourceTargetClass,
    ) -> Result<(), ValidationError> {
        let Some(id) = id else {
            return Ok(());
        };
        let entry = authoring.entries.get(id).ok_or_else(|| {
            ValidationError::UnknownDocumentationEntry {
                preview: self.id.clone(),
                entry: id.to_string(),
            }
        })?;
        let target = authoring.targets.get(entry.target_id.as_str()).copied();
        let context_matches = target.is_some_and(|target| match expected_target {
            SourceTargetClass::ExampleDeclaration => target.label == self.identity.example,
            SourceTargetClass::PageDeclaration => {
                target.owner.kind == SourceTargetOwnerKind::Page
                    && target.owner.name == self.identity.subject
            }
            SourceTargetClass::SurfaceDeclaration => {
                target.owner.kind == SourceTargetOwnerKind::Surface
                    && target.owner.name == self.identity.subject
            }
            SourceTargetClass::ComponentDeclaration => {
                target.owner.kind == SourceTargetOwnerKind::Component
                    && target.owner.name == self.identity.subject
            }
            _ => false,
        });
        if entry.class != SourceMetadataClass::Doc
            || target.map(|item| item.class) != Some(expected_target)
            || !context_matches
        {
            return Err(ValidationError::IncompatibleDocumentationEntry {
                preview: self.id.clone(),
                entry: id.to_string(),
            });
        }
        Ok(())
    }

    fn anchor_resolves(&self, anchor: &RenderNodeRef) -> bool {
        let root = match (&self.content, &anchor.root) {
            (PreviewContent::Page(snapshot), RenderRoot::Page) => Some(&snapshot.page.root),
            (PreviewContent::Page(snapshot), RenderRoot::Surface { key }) if !key.is_empty() => {
                let mut matches = snapshot
                    .surfaces
                    .iter()
                    .filter(|surface| surface.key == *key);
                let root = matches.next().map(|surface| &surface.root);
                if matches.next().is_some() { None } else { root }
            }
            (PreviewContent::Fragment(node), RenderRoot::Fragment) => Some(node),
            (
                PreviewContent::Page(_) | PreviewContent::Fragment(_),
                RenderRoot::Page | RenderRoot::Fragment | RenderRoot::Surface { .. },
            ) => None,
        };
        let Some(mut node) = root else {
            return false;
        };
        for index in &anchor.path {
            let Some(child) = node.children.get(*index) else {
                return false;
            };
            node = child;
        }
        true
    }
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

fn valid_annotation_kind(kind: &str) -> bool {
    if kind.is_empty() || kind.len() > 64 || !kind.is_ascii() {
        return false;
    }
    let bytes = kind.as_bytes();
    bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes.windows(2).all(|pair| pair != b"--")
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

impl SourceTargetClass {
    fn documentable(self) -> bool {
        !self.annotatable()
    }

    fn annotatable(self) -> bool {
        matches!(
            self,
            Self::CatalogElement
                | Self::ComponentInvocation
                | Self::IfBlock
                | Self::EachBlock
                | Self::MatchBlock
        )
    }
}

impl EditorRender {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let authoring = self.authoring.validate()?;
        let mut preview_ids = BTreeSet::new();
        let mut identities = BTreeSet::new();
        let mut by_id = BTreeMap::new();
        for preview in &self.previews {
            if !preview_ids.insert(preview.id.clone()) {
                return Err(ValidationError::DuplicatePreviewId(preview.id.clone()));
            }
            if !identities.insert(preview.identity.clone()) {
                return Err(ValidationError::DuplicatePreviewIdentity(
                    preview.identity.clone(),
                ));
            }
            let expected = stable_preview_id(&preview.identity);
            if preview.id != expected {
                return Err(ValidationError::InvalidPreviewId {
                    expected,
                    actual: preview.id.clone(),
                });
            }
            let content_matches = matches!(
                (preview.identity.kind, &preview.content),
                (PreviewKind::Page, PreviewContent::Page(_))
                    | (
                        PreviewKind::Surface | PreviewKind::Component,
                        PreviewContent::Fragment(_)
                    )
            );
            if !content_matches {
                return Err(ValidationError::ContentKindMismatch(preview.id.clone()));
            }
            let replay_matches = preview.replay.len() == preview.replay_steps.len()
                && preview
                    .replay
                    .iter()
                    .zip(&preview.replay_steps)
                    .all(|(step, expected)| {
                        step.get("label").and_then(serde_json::Value::as_str)
                            == Some(expected.as_str())
                    });
            if !replay_matches {
                return Err(ValidationError::ReplayMetadataMismatch(preview.id.clone()));
            }
            preview.validate_authoring(&authoring)?;
            by_id.insert(preview.id.clone(), preview);
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
            if !by_id.contains_key(&parent_id) {
                return Err(ValidationError::UnknownPreviewParent {
                    preview: preview.id.clone(),
                    parent: parent.clone(),
                });
            }
        }

        let mut group_ids = BTreeSet::new();
        let mut grouped = BTreeSet::new();
        for group in &self.groups {
            if !group_ids.insert(group.id.clone()) {
                return Err(ValidationError::DuplicateGroupId(group.id.clone()));
            }
            for id in &group.previews {
                let preview =
                    by_id
                        .get(id)
                        .ok_or_else(|| ValidationError::UnknownGroupedPreview {
                            group: group.id.clone(),
                            preview: id.clone(),
                        })?;
                if !grouped.insert(id.clone()) {
                    return Err(ValidationError::PreviewGroupedMoreThanOnce(id.clone()));
                }
                if preview.identity.kind != group.kind || preview.identity.subject != group.subject
                {
                    return Err(ValidationError::GroupIdentityMismatch {
                        group: group.id.clone(),
                        preview: id.clone(),
                    });
                }
            }
        }
        if let Some(id) = preview_ids.difference(&grouped).next() {
            return Err(ValidationError::UngroupedPreview(id.clone()));
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
                .expect("checked interaction graphs serialize"),
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
            "anchors": self.anchors.iter().map(render_node_ref_json).collect::<Vec<_>>(),
        })
    }
}

fn render_node_ref_json(anchor: &RenderNodeRef) -> serde_json::Value {
    let root = match &anchor.root {
        RenderRoot::Page => serde_json::json!({ "kind": "page" }),
        RenderRoot::Fragment => serde_json::json!({ "kind": "fragment" }),
        RenderRoot::Surface { key } => serde_json::json!({
            "kind": "surface",
            "key": key,
        }),
    };
    serde_json::json!({
        "root": root,
        "path": anchor.path,
    })
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

impl Preview {
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
            // Snapshot and fragment node remain the existing semantic wire
            // forms; no wrapper introduces a second view protocol.
            "content": self.content.to_json(),
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

impl PreviewContent {
    fn to_json(&self) -> serde_json::Value {
        match self {
            PreviewContent::Page(snapshot) => snapshot.to_json(),
            PreviewContent::Fragment(node) => node.to_json(),
        }
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
            RenderFreshness::Current => "current",
            RenderFreshness::Stale => "stale",
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
            Self::CatalogElement => "catalog-element",
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
            PreviewKind::Page => "page",
            PreviewKind::Surface => "surface",
            PreviewKind::Component => "component",
        }
    }
}

impl PreviewFieldGroup {
    fn as_str(self) -> &'static str {
        match self {
            PreviewFieldGroup::Properties => "properties",
            PreviewFieldGroup::PageAddress => "page-address",
            PreviewFieldGroup::ProvidedData => "provided-data",
        }
    }
}

impl PreviewFieldValue {
    fn status(&self) -> &'static str {
        match self {
            PreviewFieldValue::Ready(_) => "ready",
            PreviewFieldValue::Waiting => "waiting",
            PreviewFieldValue::Failed(_) => "failed",
        }
    }
}

impl PreviewFieldSourceKind {
    fn as_str(&self) -> &'static str {
        match self {
            PreviewFieldSourceKind::Inline => "inline",
            PreviewFieldSourceKind::Fixture { .. } => "fixture",
            PreviewFieldSourceKind::AutomaticFixture { .. } => "automatic-fixture",
        }
    }
}

impl InteractionKind {
    fn as_str(self) -> &'static str {
        match self {
            InteractionKind::Input => "input",
            InteractionKind::Observe => "observe",
        }
    }
}

fn build_authoring_metadata(
    projection: &CheckedAuthoringProjection,
    source_map: &SourceMap,
) -> Result<(AuthoringMetadata, BTreeSet<String>), BuildError> {
    projection
        .validate()
        .map_err(BuildError::InvalidAuthoring)?;
    let checked_targets = projection
        .targets
        .iter()
        .map(|target| (target.id.as_str(), target))
        .collect::<BTreeMap<_, _>>();
    for target in &projection.targets {
        if source_map.path(target.span.file) != target.file {
            return Err(BuildError::InvalidAuthoring(format!(
                "target `{}` file does not match its source span",
                target.id
            )));
        }
    }
    for entry in &projection.entries {
        let target = checked_targets[entry.target_id.as_str()];
        if entry.metadata_span.file != target.span.file {
            return Err(BuildError::InvalidAuthoring(format!(
                "metadata entry `{}` is not in its target's source file",
                entry.id
            )));
        }
    }
    let referenced_targets = projection
        .entries
        .iter()
        .map(|entry| entry.target_id.as_str())
        .collect::<BTreeSet<_>>();
    let annotation_targets = projection
        .entries
        .iter()
        .filter(|entry| entry.class == CheckedMetadataClass::Annotation)
        .map(|entry| entry.target_id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let target_files = projection
        .targets
        .iter()
        .map(|target| (target.id.as_str(), target.file.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut targets = projection
        .targets
        .iter()
        .filter(|target| referenced_targets.contains(target.id.as_str()))
        .map(|target| SourceTarget {
            id: target.id.as_str().to_string(),
            class: source_target_class(target.class),
            file: target.file.clone(),
            span: EditorSourceSpan::from_span(source_map, target.span),
            label: target.label.clone(),
            owner: SourceTargetOwner {
                kind: source_owner_kind(target.owner.kind),
                name: target.owner.name.clone(),
            },
        })
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| left.span.offset.cmp(&right.span.offset))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut entries = projection
        .entries
        .iter()
        .map(|entry| SourceMetadataEntry {
            id: entry.id.as_str().to_string(),
            class: match entry.class {
                CheckedMetadataClass::Doc => SourceMetadataClass::Doc,
                CheckedMetadataClass::Annotation => SourceMetadataClass::Annotation,
            },
            kind: entry.kind.clone(),
            text: entry.text.clone(),
            span: EditorSourceSpan::from_span(source_map, entry.metadata_span),
            target_id: entry.target_id.as_str().to_string(),
            order: entry.order as usize,
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        target_files
            .get(left.target_id.as_str())
            .cmp(&target_files.get(right.target_id.as_str()))
            .then_with(|| left.span.offset.cmp(&right.span.offset))
            .then_with(|| left.order.cmp(&right.order))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok((AuthoringMetadata { targets, entries }, annotation_targets))
}

fn source_target_class(class: CheckedTargetClass) -> SourceTargetClass {
    match class {
        CheckedTargetClass::SourceModule => SourceTargetClass::SourceModule,
        CheckedTargetClass::ComponentDeclaration => SourceTargetClass::ComponentDeclaration,
        CheckedTargetClass::PageDeclaration => SourceTargetClass::PageDeclaration,
        CheckedTargetClass::SurfaceDeclaration => SourceTargetClass::SurfaceDeclaration,
        CheckedTargetClass::PropDeclaration => SourceTargetClass::PropDeclaration,
        CheckedTargetClass::EmittedEventDeclaration => SourceTargetClass::EmittedEventDeclaration,
        CheckedTargetClass::EmittedEventParameter => SourceTargetClass::EmittedEventParameter,
        CheckedTargetClass::RouteParameter => SourceTargetClass::RouteParameter,
        CheckedTargetClass::StoreScope => SourceTargetClass::StoreScope,
        CheckedTargetClass::StateField => SourceTargetClass::StateField,
        CheckedTargetClass::EventHandler => SourceTargetClass::EventHandler,
        CheckedTargetClass::OutcomeHandler => SourceTargetClass::OutcomeHandler,
        CheckedTargetClass::HandlerParameter => SourceTargetClass::HandlerParameter,
        CheckedTargetClass::ExampleDeclaration => SourceTargetClass::ExampleDeclaration,
        CheckedTargetClass::CatalogElement => SourceTargetClass::CatalogElement,
        CheckedTargetClass::ComponentInvocation => SourceTargetClass::ComponentInvocation,
        CheckedTargetClass::IfBlock => SourceTargetClass::IfBlock,
        CheckedTargetClass::EachBlock => SourceTargetClass::EachBlock,
        CheckedTargetClass::MatchBlock => SourceTargetClass::MatchBlock,
    }
}

fn source_owner_kind(kind: CheckedOwnerKind) -> SourceTargetOwnerKind {
    match kind {
        CheckedOwnerKind::Module => SourceTargetOwnerKind::Module,
        CheckedOwnerKind::Examples => SourceTargetOwnerKind::Examples,
        CheckedOwnerKind::Component => SourceTargetOwnerKind::Component,
        CheckedOwnerKind::Page => SourceTargetOwnerKind::Page,
        CheckedOwnerKind::Surface => SourceTargetOwnerKind::Surface,
    }
}

fn build_preview_provenance(
    preview_id: &str,
    trace: EvaluationTrace,
    template_origins: &BTreeMap<TemplateAddress, uhura_check::metadata::SourceTargetId>,
    annotation_targets: &BTreeSet<String>,
) -> Result<PreviewProvenance, BuildError> {
    let mut root_ordinals = BTreeMap::<RenderRoot, usize>::new();
    for occurrence in &trace.occurrences {
        let next = root_ordinals.len();
        root_ordinals.entry(occurrence.root.clone()).or_insert(next);
    }
    let mut traced = trace.occurrences;
    traced.sort_by(|left, right| {
        root_ordinals[&left.root]
            .cmp(&root_ordinals[&right.root])
            .then_with(|| left.template.cmp(&right.template))
            .then_with(|| left.context.cmp(&right.context))
    });
    let mut occurrences = Vec::new();
    for occurrence in traced {
        let target = template_origins.get(&occurrence.template).ok_or_else(|| {
            BuildError::MissingTemplateOrigin {
                preview: preview_id.to_string(),
                template: occurrence.template.clone(),
            }
        })?;
        if !annotation_targets.contains(target.as_str()) {
            continue;
        }
        let target_id = target.as_str().to_string();
        occurrences.push(TargetOccurrence {
            id: stable_occurrence_id(
                preview_id,
                &target_id,
                &occurrence.root,
                &occurrence.context,
            ),
            target_id,
            anchors: occurrence.anchors,
        });
    }
    Ok(PreviewProvenance { occurrences })
}

fn stable_occurrence_id(
    preview_id: &str,
    target_id: &str,
    root: &RenderRoot,
    context: &EvaluationContext,
) -> String {
    hash_json(&serde_json::json!({
        "preview": preview_id,
        "target": target_id,
        "root": render_root_identity_json(root),
        "context": evaluation_context_json(context),
    }))
}

fn render_root_identity_json(root: &RenderRoot) -> serde_json::Value {
    match root {
        RenderRoot::Page => serde_json::json!({ "kind": "page" }),
        RenderRoot::Fragment => serde_json::json!({ "kind": "fragment" }),
        RenderRoot::Surface { key } => {
            serde_json::json!({ "kind": "surface", "key": key })
        }
    }
}

fn evaluation_context_json(context: &EvaluationContext) -> serde_json::Value {
    serde_json::Value::Array(
        context
            .segments
            .iter()
            .map(|segment| match segment {
                EvaluationContextSegment::ComponentCall { call } => serde_json::json!({
                    "kind": "component-call",
                    "call": template_address_json(call),
                }),
                EvaluationContextSegment::EachItem { each, key } => serde_json::json!({
                    "kind": "each-item",
                    "each": template_address_json(each),
                    "key": key,
                }),
            })
            .collect(),
    )
}

fn template_address_json(address: &TemplateAddress) -> serde_json::Value {
    serde_json::json!({
        "definition": {
            "kind": match address.definition.kind {
                DefinitionKind::Page => "page",
                DefinitionKind::Component => "component",
                DefinitionKind::Surface => "surface",
            },
            "name": address.definition.name.to_string(),
        },
        "path": address.path.iter().map(|segment| match segment {
            TemplateSegment::ElementChild { index } => {
                serde_json::json!({ "kind": "element-child", "index": index })
            }
            TemplateSegment::IfThen { index } => {
                serde_json::json!({ "kind": "if-then", "index": index })
            }
            TemplateSegment::IfElse { index } => {
                serde_json::json!({ "kind": "if-else", "index": index })
            }
            TemplateSegment::EachBody { index } => {
                serde_json::json!({ "kind": "each-body", "index": index })
            }
            TemplateSegment::MatchArm { arm, child } => {
                serde_json::json!({ "kind": "match-arm", "arm": arm, "child": child })
            }
        }).collect::<Vec<_>>(),
    })
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

fn build_preview(
    program: &uhura_core::ir::ProgramIr,
    template_origins: &BTreeMap<TemplateAddress, uhura_check::metadata::SourceTargetId>,
    annotation_targets: &BTreeSet<String>,
    checked: &ResolvedPreview,
) -> Result<Preview, BuildError> {
    let (kind, subject) = match &checked.subject {
        SubjectKind::Page { route } => (PreviewKind::Page, route.to_string()),
        SubjectKind::Surface { name, .. } => (PreviewKind::Surface, name.to_string()),
        SubjectKind::Component { name } => (PreviewKind::Component, name.to_string()),
    };
    let identity = PreviewIdentity {
        kind,
        subject,
        example: checked.example.clone(),
    };
    let id = stable_preview_id(&identity);
    let (content, trace) = match &checked.payload {
        PreviewPayload::Page { u, x, .. } => eval_view_with_trace(program, u, x)
            .map(|(snapshot, trace)| (PreviewContent::Page(snapshot), trace))
            .map_err(|error| BuildError::Evaluation {
                preview: id.clone(),
                message: error.to_string(),
            })?,
        PreviewPayload::Fragment {
            surface,
            name,
            props,
            state,
            x,
        } => {
            let definition = if *surface {
                program.surfaces.get(name)
            } else {
                program.components.get(name)
            }
            .ok_or_else(|| BuildError::MissingDefinition {
                preview: id.clone(),
                definition: name.to_string(),
            })?;
            let definition_address = DefinitionAddress::new(
                if *surface {
                    DefinitionKind::Surface
                } else {
                    DefinitionKind::Component
                },
                name.clone(),
            );
            eval_fragment_with_trace(program, &definition_address, definition, props, state, x)
                .map(|(node, trace)| (PreviewContent::Fragment(node), trace))
                .map_err(|error| BuildError::Evaluation {
                    preview: id.clone(),
                    message: error.to_string(),
                })?
        }
    };
    let mut interactions = Vec::new();
    collect_content_interactions(&content, &mut interactions);
    let provenance = build_preview_provenance(&id, trace, template_origins, annotation_targets)?;
    Ok(Preview {
        id,
        identity,
        source_file: checked.source_file.clone(),
        is_default: checked.is_default,
        pinned: checked.pinned,
        derived: checked.derived,
        in_flight: checked.in_flight,
        from: checked.from.clone(),
        replay_steps: checked.replay_steps.clone(),
        replay: checked.replay.iter().map(|step| step.to_json()).collect(),
        note: checked.note.clone(),
        data: checked.data.iter().map(build_field).collect(),
        interactions,
        documentation: PreviewDocumentation {
            declaration_doc_id: checked
                .declaration_doc_id
                .as_ref()
                .map(|value| value.as_str().to_string()),
            example_doc_id: checked
                .example_doc_id
                .as_ref()
                .map(|value| value.as_str().to_string()),
        },
        provenance,
        content,
    })
}

fn build_field(field: &uhura_check::preview::PreviewData) -> PreviewField {
    PreviewField {
        group: match field.kind {
            PreviewDataKind::Property => PreviewFieldGroup::Properties,
            PreviewDataKind::PageAddress => PreviewFieldGroup::PageAddress,
            PreviewDataKind::ProvidedData => PreviewFieldGroup::ProvidedData,
        },
        name: field.name.to_string(),
        key: field.key.clone(),
        value: match &field.value {
            PreviewDataValue::Ready(value) => PreviewFieldValue::Ready(value.clone()),
            PreviewDataValue::Waiting => PreviewFieldValue::Waiting,
            PreviewDataValue::Failed(reason) => PreviewFieldValue::Failed(reason.clone()),
        },
        source: field.origin.as_ref().map(build_field_source),
    }
}

fn build_field_source(origin: &PreviewOrigin) -> PreviewFieldSource {
    PreviewFieldSource {
        kind: match &origin.source {
            CheckedPreviewSource::Inline => PreviewFieldSourceKind::Inline,
            CheckedPreviewSource::Fixture { fixture, path } => PreviewFieldSourceKind::Fixture {
                fixture: fixture.clone(),
                path: path.clone(),
            },
            CheckedPreviewSource::AutomaticFixture { fixture, path } => {
                PreviewFieldSourceKind::AutomaticFixture {
                    fixture: fixture.clone(),
                    path: path.clone(),
                }
            }
        },
        declared_in: origin.declared_in.clone(),
        timeline: origin.timeline,
    }
}

fn collect_content_interactions(content: &PreviewContent, output: &mut Vec<Interaction>) {
    match content {
        PreviewContent::Page(snapshot) => {
            collect_node_interactions(&snapshot.page.root, output);
            for surface in &snapshot.surfaces {
                collect_descriptor(&surface.root, "surface", &surface.dismiss, output);
                collect_node_interactions(&surface.root, output);
            }
        }
        PreviewContent::Fragment(node) => collect_node_interactions(node, output),
    }
}

fn collect_node_interactions(node: &Node, output: &mut Vec<Interaction>) {
    for descriptor in &node.on {
        collect_descriptor(node, node.element.as_str(), descriptor, output);
    }
    for child in &node.children {
        collect_node_interactions(child, output);
    }
}

fn collect_descriptor(
    node: &Node,
    element: &str,
    descriptor: &Descriptor,
    output: &mut Vec<Interaction>,
) {
    output.push(Interaction {
        node_key: node.key.clone(),
        element: element.to_string(),
        kind: match descriptor.kind {
            DescriptorKind::Input => InteractionKind::Input,
            DescriptorKind::Observe => InteractionKind::Observe,
        },
        event: descriptor.event.to_string(),
        emit: descriptor.emit.to_string(),
        scope: descriptor.scope.clone(),
        payload: descriptor.payload.clone(),
        carries: descriptor
            .carries
            .iter()
            .map(|(name, shape)| (name.to_string(), shape.clone()))
            .collect(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_base::{Ident, SourceMap};
    use uhura_check::LockStatus;
    use uhura_check::lower::Lowered;
    use uhura_check::metadata::{
        AuthoringProjection as CheckedProjection, MetadataClass as CheckedClass,
        SourceMetadataEntry as CheckedEntry, SourceOwner as CheckedOwner,
        SourceOwnerKind as CheckedOwnerKind, SourceSyntaxAddress, SourceSyntaxSegment,
        SourceTarget as CheckedTarget, SourceTargetClass as CheckedClassTarget,
        SourceTargetId as CheckedTargetId,
    };
    use uhura_check::preview::{PreviewData, PreviewSource};
    use uhura_core::ir::{
        CatalogPin, DefIr, ElementEventBindingIr, ElementEventIr, ElementIr, EventKeyIr,
        EventKindIr, HandlerIr, NodeIr, ProgramIr, StmtIr,
    };
    use uhura_core::state::{Counters, NavEntry, Projections, UiState};

    fn ident(value: &str) -> Ident {
        Ident::new(value).expect("test identifier")
    }

    fn diagnostics(message: &str) -> serde_json::Value {
        serde_json::json!({
            "format": "uhura-diagnostics",
            "version": 0,
            "summary": { "errors": 1, "warnings": 0 },
            "diagnostics": [{
                "code": "UH9000",
                "rule": "editor/test",
                "severity": "error",
                "message": message,
            }],
        })
    }

    fn definition(element: &str) -> DefIr {
        DefIr {
            modality: None,
            props: Vec::new(),
            emits: Vec::new(),
            params: Vec::new(),
            state: BTreeMap::new(),
            events: BTreeMap::new(),
            handlers: Vec::new(),
            root: NodeIr::Element(ElementIr {
                element: ident(element),
                ord: 0,
                class: None,
                props: Vec::new(),
                events: vec![ElementEventBindingIr {
                    event: ident("press"),
                    emit: ident("activated"),
                    args: Vec::new(),
                }],
                text: Vec::new(),
                children: Vec::new(),
            }),
        }
    }

    fn clean_output() -> CheckOutput {
        let home = ident("home");
        let sheet = ident("comments-sheet");
        let card = ident("post-card");
        let button = ident("button");
        let program = ProgramIr {
            protocol: uhura_core::ir::IR_PROTOCOL.to_string(),
            app: ident("model-test"),
            entry: home.clone(),
            catalog: CatalogPin {
                name: ident("test-catalog"),
                version: "0".to_string(),
                hash: "catalog-hash".to_string(),
            },
            ports: BTreeMap::new(),
            projections: BTreeMap::new(),
            element_events: BTreeMap::from([(
                button,
                BTreeMap::from([(
                    ident("press"),
                    ElementEventIr {
                        kind: EventKindIr::Input,
                        carries: BTreeMap::new(),
                    },
                )]),
            )]),
            element_props: BTreeMap::new(),
            routes: BTreeMap::new(),
            pages: BTreeMap::from([(home.clone(), definition("button"))]),
            components: BTreeMap::from([(card.clone(), definition("button"))]),
            surfaces: BTreeMap::from([(sheet.clone(), definition("button"))]),
        };
        let root_address = SourceSyntaxAddress(vec![
            SourceSyntaxSegment::Definition,
            SourceSyntaxSegment::Markup,
            SourceSyntaxSegment::Item(0),
        ]);
        let template_origins = BTreeMap::from([
            (
                TemplateAddress::root(DefinitionAddress::new(DefinitionKind::Page, home.clone())),
                CheckedTargetId::from_parts(
                    "pages/home.uhura",
                    CheckedClassTarget::CatalogElement,
                    &root_address,
                ),
            ),
            (
                TemplateAddress::root(DefinitionAddress::new(
                    DefinitionKind::Component,
                    card.clone(),
                )),
                CheckedTargetId::from_parts(
                    "components/post-card.uhura",
                    CheckedClassTarget::CatalogElement,
                    &root_address,
                ),
            ),
            (
                TemplateAddress::root(DefinitionAddress::new(
                    DefinitionKind::Surface,
                    sheet.clone(),
                )),
                CheckedTargetId::from_parts(
                    "surfaces/comments-sheet.uhura",
                    CheckedClassTarget::CatalogElement,
                    &root_address,
                ),
            ),
        ]);
        let fields = vec![
            PreviewData {
                kind: PreviewDataKind::Property,
                name: ident("count"),
                key: None,
                value: PreviewDataValue::Ready(Value::Int(3)),
                origin: Some(PreviewOrigin {
                    declared_in: Some("base".to_string()),
                    source: PreviewSource::Inline,
                    timeline: false,
                }),
            },
            PreviewData {
                kind: PreviewDataKind::ProvidedData,
                name: ident("feed"),
                key: None,
                value: PreviewDataValue::Waiting,
                origin: Some(PreviewOrigin {
                    declared_in: None,
                    source: PreviewSource::AutomaticFixture {
                        fixture: "sample".to_string(),
                        path: vec!["boot".to_string(), "feed".to_string()],
                    },
                    timeline: false,
                }),
            },
            PreviewData {
                kind: PreviewDataKind::PageAddress,
                name: ident("user"),
                key: Some(Value::Id("user-1".to_string())),
                value: PreviewDataValue::Failed("offline".to_string()),
                origin: Some(PreviewOrigin {
                    declared_in: Some("derived".to_string()),
                    source: PreviewSource::Fixture {
                        fixture: "sample".to_string(),
                        path: vec!["profiles".to_string(), "user-1".to_string()],
                    },
                    timeline: true,
                }),
            },
        ];
        let page_state = UiState {
            rev: 4,
            nav: vec![NavEntry {
                serial: 1,
                route: home.clone(),
                params: BTreeMap::new(),
                state: BTreeMap::new(),
            }],
            surfaces: Vec::new(),
            pending: BTreeMap::new(),
            counters: Counters {
                page_serial: 1,
                ..Counters::default()
            },
        };
        let previews = vec![
            ResolvedPreview {
                subject: SubjectKind::Page {
                    route: home.clone(),
                },
                source_file: "pages/home.uhura".to_string(),
                example: "default".to_string(),
                is_default: true,
                pinned: false,
                derived: false,
                in_flight: 0,
                from: None,
                replay_steps: Vec::new(),
                replay: Vec::new(),
                note: Some("Entry page".to_string()),
                data: fields,
                declaration_doc_id: None,
                example_doc_id: None,
                payload: PreviewPayload::Page {
                    route: home,
                    u: page_state,
                    x: Projections::default(),
                },
            },
            ResolvedPreview {
                subject: SubjectKind::Surface {
                    name: sheet.clone(),
                    modality: Some("sheet".to_string()),
                },
                source_file: "surfaces/comments-sheet.uhura".to_string(),
                example: "open".to_string(),
                is_default: true,
                pinned: true,
                derived: false,
                in_flight: 0,
                from: None,
                replay_steps: Vec::new(),
                replay: Vec::new(),
                note: None,
                data: Vec::new(),
                declaration_doc_id: None,
                example_doc_id: None,
                payload: PreviewPayload::Fragment {
                    surface: true,
                    name: sheet,
                    props: BTreeMap::new(),
                    state: BTreeMap::new(),
                    x: Projections::default(),
                },
            },
            ResolvedPreview {
                subject: SubjectKind::Component { name: card.clone() },
                source_file: "components/post-card.uhura".to_string(),
                example: "liked".to_string(),
                is_default: false,
                pinned: false,
                derived: true,
                in_flight: 2,
                from: None,
                replay_steps: vec!["activated".to_string()],
                replay: vec![uhura_check::replay::ReplayStep {
                    label: "activated".to_string(),
                    kind: uhura_check::replay::ReplayStepKind::Semantic,
                    payload: serde_json::json!({}),
                    dispatch: Some(uhura_check::replay::ReplayDispatch {
                        scope: "fragment:0".to_string(),
                        definition: "post-card".to_string(),
                        on: "activated".to_string(),
                        guards: vec![uhura_check::replay::ReplayGuard {
                            handler: 0,
                            result: "satisfied",
                        }],
                        selected: Some(0),
                        aborted: None,
                    }),
                    effects: uhura_check::replay::ReplayEffects::default(),
                }],
                note: None,
                data: Vec::new(),
                declaration_doc_id: None,
                example_doc_id: None,
                payload: PreviewPayload::Fragment {
                    surface: false,
                    name: card,
                    props: BTreeMap::new(),
                    state: BTreeMap::new(),
                    x: Projections::default(),
                },
            },
        ];
        CheckOutput {
            diagnostics: Vec::new(),
            source_map: SourceMap::new(),
            authoring: CheckedAuthoringProjection::default(),
            lowered: Some(Lowered {
                program,
                spans: BTreeMap::new(),
                template_origins,
            }),
            previews,
            stylesheet: ".app { color: black; }".to_string(),
            lock_computed: String::new(),
            lock_status: LockStatus::Match,
        }
    }

    fn authored_output() -> CheckOutput {
        let mut output = clean_output();
        let file = output
            .source_map
            .add("pages/home.uhura", "abcdefghijklmnop");
        let examples_file = output
            .source_map
            .add("pages/home.examples.uhura", "abcdefghijkl");
        let owner = CheckedOwner {
            kind: CheckedOwnerKind::Page,
            name: "home".to_string(),
        };
        let declaration = CheckedTarget::new(
            CheckedClassTarget::PageDeclaration,
            "pages/home.uhura".to_string(),
            Span::new(file, 4, 8),
            SourceSyntaxAddress(vec![SourceSyntaxSegment::Definition]),
            owner.clone(),
            "page home".to_string(),
        );
        let element = CheckedTarget::new(
            CheckedClassTarget::CatalogElement,
            "pages/home.uhura".to_string(),
            Span::new(file, 12, 16),
            SourceSyntaxAddress(vec![
                SourceSyntaxSegment::Definition,
                SourceSyntaxSegment::Markup,
                SourceSyntaxSegment::Item(0),
            ]),
            owner.clone(),
            "button".to_string(),
        );
        let undocumented_prop = CheckedTarget::new(
            CheckedClassTarget::PropDeclaration,
            "pages/home.uhura".to_string(),
            Span::new(file, 2, 3),
            SourceSyntaxAddress(vec![
                SourceSyntaxSegment::Definition,
                SourceSyntaxSegment::Props,
                SourceSyntaxSegment::Item(0),
            ]),
            owner,
            "title".to_string(),
        );
        let doc = CheckedEntry::new(
            CheckedClass::Doc,
            "doc".to_string(),
            "The home page.".to_string(),
            Span::new(file, 0, 4),
            declaration.id.clone(),
            0,
        );
        let annotation = CheckedEntry::new(
            CheckedClass::Annotation,
            "review-note".to_string(),
            "The primary action.".to_string(),
            Span::new(file, 8, 12),
            element.id.clone(),
            0,
        );
        let example = CheckedTarget::new(
            CheckedClassTarget::ExampleDeclaration,
            "pages/home.examples.uhura".to_string(),
            Span::new(examples_file, 4, 8),
            SourceSyntaxAddress(vec![
                SourceSyntaxSegment::Examples,
                SourceSyntaxSegment::Item(0),
            ]),
            CheckedOwner {
                kind: CheckedOwnerKind::Examples,
                name: "pages/home.examples.uhura".to_string(),
            },
            "default".to_string(),
        );
        let example_doc = CheckedEntry::new(
            CheckedClass::Doc,
            "doc".to_string(),
            "The default example.".to_string(),
            Span::new(examples_file, 0, 4),
            example.id.clone(),
            0,
        );
        output.previews[0].declaration_doc_id = Some(doc.id.clone());
        output.previews[0].example_doc_id = Some(example_doc.id.clone());
        output.authoring = CheckedProjection {
            targets: vec![undocumented_prop, element.clone(), example, declaration],
            entries: vec![annotation, doc, example_doc],
        };
        output.lowered.as_mut().unwrap().template_origins.insert(
            TemplateAddress::root(DefinitionAddress::new(DefinitionKind::Page, ident("home"))),
            element.id,
        );
        output
    }

    #[test]
    fn builder_serializes_deterministically_and_keeps_semantic_content() {
        let assets = BTreeMap::from([(
            "avatar".to_string(),
            Asset {
                data_uri: "data:image/jpeg;base64,AA==".to_string(),
                alt: "Avatar".to_string(),
            },
        )]);
        let first = build_current_state(9, &authored_output(), assets.clone()).unwrap();
        let second = build_current_state(9, &authored_output(), assets).unwrap();
        let first_json = first.to_canonical_string().unwrap();
        assert_eq!(first_json, second.to_canonical_string().unwrap());
        assert_eq!(first.to_json()["protocol"], EDITOR_STATE_PROTOCOL);
        assert_eq!(first.to_json()["sourceRevision"], 9);
        assert_eq!(first.to_json()["render"]["freshness"], "current");
        assert!(
            first.to_json()["render"].get("icons").is_none(),
            "glyph geometry is renderer-owned and absent from EditorState"
        );
        assert_eq!(
            first.to_json()["render"]["previews"][0]["content"]["protocol"],
            uhura_core::view::VIEW_PROTOCOL
        );
        assert!(
            first.to_json()["render"]["previews"][1]["content"]
                .get("protocol")
                .is_none(),
            "a fragment is the Node wire form directly"
        );
        assert!(!first_json.contains("<path"));
        assert!(!first_json.contains("<style"));

        let mut fixture = first;
        let render = fixture.render.as_mut().unwrap();
        render.assets.clear();
        assert_eq!(
            fixture.to_canonical_string().unwrap(),
            include_str!("../tests/fixtures/editor-state.json").trim()
        );
    }

    #[test]
    fn builder_covers_all_preview_kinds_statuses_provenance_and_interactions() {
        let state = build_current_state(3, &clean_output(), BTreeMap::new()).unwrap();
        let render = state.render.unwrap();
        assert_eq!(
            render
                .previews
                .iter()
                .map(|preview| preview.identity.kind)
                .collect::<Vec<_>>(),
            vec![
                PreviewKind::Page,
                PreviewKind::Surface,
                PreviewKind::Component
            ]
        );
        assert_eq!(render.groups.len(), 3);
        assert!(matches!(
            render.previews[0].data[0].value,
            PreviewFieldValue::Ready(Value::Int(3))
        ));
        assert!(matches!(
            render.previews[0].data[1].value,
            PreviewFieldValue::Waiting
        ));
        assert!(matches!(
            render.previews[0].data[2].value,
            PreviewFieldValue::Failed(ref reason) if reason == "offline"
        ));
        assert!(matches!(
            render.previews[0].data[2]
                .source
                .as_ref()
                .map(|source| &source.kind),
            Some(PreviewFieldSourceKind::Fixture { fixture, path })
                if fixture == "sample" && path == &["profiles", "user-1"]
        ));
        assert!(
            render
                .previews
                .iter()
                .all(|preview| !preview.interactions.is_empty())
        );
        assert_eq!(render.previews[0].interactions[0].emit, "activated");
        assert_eq!(render.previews[2].replay[0]["dispatch"]["selected"], 0);
    }

    #[test]
    fn builder_embeds_the_interaction_graph_of_the_same_checked_program() {
        let mut output = clean_output();
        let program = &mut output.lowered.as_mut().unwrap().program;
        let home = program.pages.get_mut(&ident("home")).unwrap();
        home.handlers.push(HandlerIr {
            on: EventKeyIr::Semantic {
                event: ident("comments-requested"),
            },
            params: Vec::new(),
            guard: None,
            body: vec![
                StmtIr::OpenSurface {
                    surface: ident("comments-sheet"),
                    args: Vec::new(),
                },
                StmtIr::Navigate {
                    route: ident("home"),
                    args: Vec::new(),
                },
            ],
        });

        let state = build_current_state(3, &output, BTreeMap::new()).unwrap();
        let graph = &state.to_json()["render"]["interactionGraph"];
        assert_eq!(
            graph["protocol"],
            interaction_graph::INTERACTION_GRAPH_PROTOCOL
        );
        assert_eq!(graph["app"], "model-test");
        assert_eq!(graph["entry"], "page:home");
        let nodes = graph["nodes"].as_array().unwrap();
        assert_eq!(
            nodes
                .iter()
                .map(|node| node["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "dynamic:opener",
                "dynamic:previous-page",
                "page:home",
                "surface:comments-sheet",
            ],
        );
        let edges = graph["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0]["kind"], "present");
        assert_eq!(edges[0]["from"], "page:home");
        assert_eq!(edges[0]["to"], "surface:comments-sheet");
        assert_eq!(edges[0]["event"], "comments-requested");
        assert_eq!(edges[1]["kind"], "navigate");
        assert_eq!(edges[1]["from"], "page:home");
        assert_eq!(edges[1]["to"], "page:home");
    }

    #[test]
    fn builder_reports_missing_template_origin_with_preview_context() {
        let mut output = clean_output();
        let template =
            TemplateAddress::root(DefinitionAddress::new(DefinitionKind::Page, ident("home")));
        output
            .lowered
            .as_mut()
            .unwrap()
            .template_origins
            .remove(&template);

        let error = build_current_state(5, &output, BTreeMap::new()).unwrap_err();
        assert_eq!(
            error,
            BuildError::MissingTemplateOrigin {
                preview: "page/home/default".to_string(),
                template,
            }
        );
        assert!(error.to_string().contains("page/home/default"));
        assert!(error.to_string().contains("no source origin"));
    }

    #[test]
    fn builder_joins_checked_metadata_docs_and_traced_occurrences() {
        let output = authored_output();
        assert_eq!(output.authoring.targets.len(), 4);
        let state = build_current_state(5, &output, BTreeMap::new()).unwrap();
        let render = state.render.as_ref().unwrap();
        assert_eq!(
            render.authoring.targets.len(),
            3,
            "the wire omits checker targets without metadata"
        );
        assert_eq!(render.authoring.entries.len(), 3);
        assert_eq!(render.previews[0].source_file, "pages/home.uhura");
        let declaration_doc = render
            .authoring
            .entries
            .iter()
            .find(|entry| {
                render.authoring.targets.iter().any(|target| {
                    target.id == entry.target_id
                        && target.class == SourceTargetClass::PageDeclaration
                })
            })
            .unwrap();
        let example_doc = render
            .authoring
            .entries
            .iter()
            .find(|entry| {
                render.authoring.targets.iter().any(|target| {
                    target.id == entry.target_id
                        && target.class == SourceTargetClass::ExampleDeclaration
                })
            })
            .unwrap();
        assert_eq!(
            render.previews[0].documentation.declaration_doc_id,
            Some(declaration_doc.id.clone())
        );
        assert_eq!(
            render.previews[0].documentation.example_doc_id,
            Some(example_doc.id.clone())
        );
        let occurrence = &render.previews[0].provenance.occurrences[0];
        assert_eq!(
            render
                .authoring
                .targets
                .iter()
                .find(|target| target.id == occurrence.target_id)
                .map(|target| target.class),
            Some(SourceTargetClass::CatalogElement)
        );
        assert_eq!(
            occurrence.anchors,
            vec![RenderNodeRef {
                root: RenderRoot::Page,
                path: Vec::new(),
            }]
        );

        let second = build_current_state(5, &authored_output(), BTreeMap::new()).unwrap();
        assert_eq!(
            occurrence.id,
            second.render.unwrap().previews[0].provenance.occurrences[0].id,
            "occurrence identity is deterministic and excludes DOM/runtime keys"
        );

        let stale = EditorState::stale(6, diagnostics("rejected"), render.clone()).unwrap();
        assert_eq!(stale.render.unwrap().authoring, render.authoring);
    }

    #[test]
    fn validation_rejects_cross_preview_docs_and_invalid_semantic_anchors() {
        let mut state = build_current_state(5, &authored_output(), BTreeMap::new()).unwrap();
        state.render.as_mut().unwrap().previews[0].source_file = "../home.uhura".to_string();
        assert!(matches!(
            state.validate(),
            Err(ValidationError::InvalidAuthoringField {
                field: "sourceFile",
                ..
            })
        ));

        let mut state = build_current_state(5, &authored_output(), BTreeMap::new()).unwrap();
        state
            .render
            .as_mut()
            .unwrap()
            .authoring
            .targets
            .iter_mut()
            .find(|target| target.class == SourceTargetClass::PageDeclaration)
            .unwrap()
            .owner
            .name = "another-page".to_string();
        assert!(matches!(
            state.validate(),
            Err(ValidationError::IncompatibleDocumentationEntry { .. })
        ));

        let mut state = build_current_state(5, &authored_output(), BTreeMap::new()).unwrap();
        state
            .render
            .as_mut()
            .unwrap()
            .authoring
            .targets
            .iter_mut()
            .find(|target| target.class == SourceTargetClass::ExampleDeclaration)
            .unwrap()
            .label = "another-example".to_string();
        assert!(matches!(
            state.validate(),
            Err(ValidationError::IncompatibleDocumentationEntry { .. })
        ));

        let mut state = build_current_state(5, &authored_output(), BTreeMap::new()).unwrap();
        state.render.as_mut().unwrap().previews[0]
            .provenance
            .occurrences[0]
            .anchors[0]
            .path
            .push(9);
        assert!(matches!(
            state.validate(),
            Err(ValidationError::InvalidOccurrenceAnchor { .. })
        ));

        let mut state = build_current_state(5, &authored_output(), BTreeMap::new()).unwrap();
        state.render.as_mut().unwrap().previews[0]
            .provenance
            .occurrences[0]
            .anchors
            .clear();
        assert!(
            state.validate().is_ok(),
            "evaluated empty occurrences remain valid"
        );

        let mut state = build_current_state(5, &authored_output(), BTreeMap::new()).unwrap();
        state
            .render
            .as_mut()
            .unwrap()
            .authoring
            .entries
            .retain(|entry| entry.class != SourceMetadataClass::Annotation);
        assert!(matches!(
            state.validate(),
            Err(ValidationError::InvalidAuthoringField {
                field: "entries",
                ..
            })
        ));
    }

    #[test]
    fn annotation_kind_validation_matches_the_rfc_lower_kebab_grammar() {
        for kind in [
            "a".to_string(),
            "a0".to_string(),
            "a-0".to_string(),
            "review-note".to_string(),
            "a".repeat(64),
        ] {
            assert!(valid_annotation_kind(&kind), "expected valid kind `{kind}`");
        }
        for kind in [
            String::new(),
            "0note".to_string(),
            "Review".to_string(),
            "review_note".to_string(),
            "-note".to_string(),
            "note-".to_string(),
            "note--later".to_string(),
            "nöté".to_string(),
            "a".repeat(65),
        ] {
            assert!(
                !valid_annotation_kind(&kind),
                "expected invalid kind `{kind}`"
            );
        }
    }

    #[test]
    fn validation_rejects_a_replay_parent_outside_the_same_subject() {
        let mut render = build_render(3, &clean_output(), BTreeMap::new()).unwrap();
        render.previews[0].from = Some("missing".to_string());
        assert_eq!(
            render.validate(),
            Err(ValidationError::UnknownPreviewParent {
                preview: "page/home/default".to_string(),
                parent: "missing".to_string(),
            })
        );
    }

    #[test]
    fn validation_rejects_replay_detail_and_label_drift() {
        let mut render = build_render(3, &clean_output(), BTreeMap::new()).unwrap();
        render.previews[2].replay_steps[0] = "different-label".to_string();
        assert_eq!(
            render.validate(),
            Err(ValidationError::ReplayMetadataMismatch(
                "component/post-card/liked".to_string(),
            ))
        );

        let mut render = build_render(3, &clean_output(), BTreeMap::new()).unwrap();
        render.previews[2].replay.clear();
        assert_eq!(
            render.validate(),
            Err(ValidationError::ReplayMetadataMismatch(
                "component/post-card/liked".to_string(),
            ))
        );
    }

    #[test]
    fn validation_rejects_protocol_and_revision_confusion() {
        let mut current = build_current_state(8, &clean_output(), BTreeMap::new()).unwrap();
        current.protocol = "uhura-editor-state/99".to_string();
        assert!(matches!(
            current.validate(),
            Err(ValidationError::UnsupportedProtocol(_))
        ));

        let render = build_render(8, &clean_output(), BTreeMap::new()).unwrap();
        assert!(matches!(
            EditorState::current(9, serde_json::Value::Null, render.clone()),
            Err(ValidationError::CurrentRevisionMismatch {
                source: 9,
                render: 8
            })
        ));
        assert!(matches!(
            EditorState::stale(8, diagnostics("rejected"), render),
            Err(ValidationError::StaleRevisionNotOlder {
                source: 8,
                render: 8
            })
        ));

        assert_eq!(
            EditorState::cold_invalid(1, serde_json::json!([])),
            Err(ValidationError::InvalidDiagnosticsEnvelope)
        );
        assert_eq!(
            EditorState::cold_invalid(1, serde_json::json!({"diagnostics": []})),
            Err(ValidationError::InvalidDiagnosticsEnvelope)
        );
    }

    #[test]
    fn cold_invalid_state_has_no_render_and_stale_state_is_explicit() {
        let cold = EditorState::cold_invalid(11, diagnostics("cold invalid")).unwrap();
        assert!(cold.validate().is_ok());
        assert_eq!(cold.to_json()["render"], serde_json::Value::Null);

        let render = build_render(10, &clean_output(), BTreeMap::new()).unwrap();
        let stale = EditorState::stale(11, diagnostics("stale"), render)
            .expect("older render is valid last-renderable state");
        assert_eq!(stale.to_json()["render"]["revision"], 10);
        assert_eq!(stale.to_json()["render"]["freshness"], "stale");
    }

    #[test]
    fn zero_is_not_a_source_or_render_revision() {
        assert_eq!(
            EditorState::cold_invalid(0, diagnostics("zero")),
            Err(ValidationError::RevisionMustBePositive {
                field: "sourceRevision"
            })
        );

        let state = build_current_state(0, &clean_output(), BTreeMap::new()).unwrap_err();
        assert_eq!(
            state,
            BuildError::InvalidState(ValidationError::RevisionMustBePositive {
                field: "sourceRevision"
            })
        );

        let render = build_render(0, &clean_output(), BTreeMap::new()).unwrap_err();
        assert_eq!(
            render,
            BuildError::InvalidState(ValidationError::RevisionMustBePositive {
                field: "render.revision"
            })
        );

        let mut render = build_render(1, &clean_output(), BTreeMap::new()).unwrap();
        render.revision = 0;
        assert_eq!(
            EditorState::current(1, serde_json::Value::Null, render),
            Err(ValidationError::RevisionMustBePositive {
                field: "render.revision"
            })
        );
    }
}
