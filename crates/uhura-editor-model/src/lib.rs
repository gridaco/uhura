//! Versioned, browser-neutral Editor read model.
//!
//! This crate is the boundary between checked Uhura projects and the web
//! application. It evaluates every resolved example into semantic
//! `uhura-core` view data and serializes one immutable, versioned state. It
//! deliberately owns no HTML, CSS, DOM identifiers, browser event handling,
//! filesystem access, or transport.

pub mod icons;

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Value, has_errors, to_canonical_json, to_envelope};
use uhura_check::CheckOutput;
use uhura_check::preview::{
    PreviewDataKind, PreviewDataValue, PreviewOrigin, PreviewPayload,
    PreviewSource as CheckedPreviewSource, ResolvedPreview,
};
use uhura_check::resolve::SubjectKind;
use uhura_core::eval::{eval_fragment, eval_view};
use uhura_core::view::{Descriptor, DescriptorKind, Node, Snapshot};

pub const EDITOR_STATE_PROTOCOL: &str = "uhura-editor-state/0";

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
    pub groups: Vec<PreviewGroup>,
    pub previews: Vec<Preview>,
    pub stylesheet: String,
    pub icons: BTreeMap<String, icons::Icon>,
    pub assets: BTreeMap<String, Asset>,
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
    pub is_default: bool,
    pub pinned: bool,
    pub derived: bool,
    pub in_flight: usize,
    pub from: Option<String>,
    pub note: Option<String>,
    pub data: Vec<PreviewField>,
    pub interactions: Vec<Interaction>,
    pub content: PreviewContent,
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
    MissingDefinition { preview: String, definition: String },
    Evaluation { preview: String, message: String },
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
            BuildError::Evaluation { preview, message } => {
                write!(f, "could not evaluate preview `{preview}`: {message}")
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
    RevisionMustBePositive { field: &'static str },
    CurrentRevisionMismatch { source: u64, render: u64 },
    StaleRevisionNotOlder { source: u64, render: u64 },
    DuplicatePreviewId(String),
    DuplicatePreviewIdentity(PreviewIdentity),
    InvalidPreviewId { expected: String, actual: String },
    DuplicateGroupId(String),
    UnknownGroupedPreview { group: String, preview: String },
    PreviewGroupedMoreThanOnce(String),
    UngroupedPreview(String),
    GroupIdentityMismatch { group: String, preview: String },
    ContentKindMismatch(String),
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

    let mut previews = Vec::with_capacity(output.previews.len());
    for checked in &output.previews {
        previews.push(build_preview(program, checked)?);
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
        groups,
        previews,
        stylesheet: output.stylesheet.clone(),
        icons: icons::table(),
        assets,
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

impl EditorRender {
    pub fn validate(&self) -> Result<(), ValidationError> {
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
            by_id.insert(preview.id.clone(), preview);
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
            "groups": self.groups.iter().map(PreviewGroup::to_json).collect::<Vec<_>>(),
            "previews": self.previews.iter().map(Preview::to_json).collect::<Vec<_>>(),
            "stylesheet": self.stylesheet,
            "icons": self.icons.iter().map(|(name, icon)| {
                (name.clone(), icon.to_json())
            }).collect::<serde_json::Map<_, _>>(),
            "assets": self.assets.iter().map(|(id, asset)| {
                (id.clone(), asset.to_json())
            }).collect::<serde_json::Map<_, _>>(),
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

impl Preview {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "identity": self.identity.to_json(),
            "default": self.is_default,
            "pinned": self.pinned,
            "derived": self.derived,
            "inFlight": self.in_flight,
            "from": self.from,
            "note": self.note,
            "data": self.data.iter().map(PreviewField::to_json).collect::<Vec<_>>(),
            "interactions": self.interactions.iter().map(Interaction::to_json).collect::<Vec<_>>(),
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
    let content = match &checked.payload {
        PreviewPayload::Page { u, x, .. } => eval_view(program, u, x)
            .map(PreviewContent::Page)
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
            eval_fragment(program, definition, props, state, x)
                .map(PreviewContent::Fragment)
                .map_err(|error| BuildError::Evaluation {
                    preview: id.clone(),
                    message: error.to_string(),
                })?
        }
    };
    let mut interactions = Vec::new();
    collect_content_interactions(&content, &mut interactions);
    Ok(Preview {
        id,
        identity,
        is_default: checked.is_default,
        pinned: checked.pinned,
        derived: checked.derived,
        in_flight: checked.in_flight,
        from: checked.from.clone(),
        note: checked.note.clone(),
        data: checked.data.iter().map(build_field).collect(),
        interactions,
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
    use uhura_check::preview::{PreviewData, PreviewSource};
    use uhura_core::ir::{
        CatalogPin, DefIr, ElementEventBindingIr, ElementEventIr, ElementIr, EventKindIr, NodeIr,
        ProgramIr,
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
                example: "default".to_string(),
                is_default: true,
                pinned: false,
                derived: false,
                in_flight: 0,
                from: None,
                note: Some("Entry page".to_string()),
                data: fields,
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
                example: "open".to_string(),
                is_default: true,
                pinned: true,
                derived: false,
                in_flight: 0,
                from: None,
                note: None,
                data: Vec::new(),
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
                example: "liked".to_string(),
                is_default: false,
                pinned: false,
                derived: true,
                in_flight: 2,
                from: Some("default".to_string()),
                note: None,
                data: Vec::new(),
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
            lowered: Some(Lowered {
                program,
                spans: BTreeMap::new(),
            }),
            previews,
            stylesheet: ".app { color: black; }".to_string(),
            lock_computed: String::new(),
            lock_status: LockStatus::Match,
        }
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
        let first = build_current_state(9, &clean_output(), assets.clone()).unwrap();
        let second = build_current_state(9, &clean_output(), assets).unwrap();
        let first_json = first.to_canonical_string().unwrap();
        assert_eq!(first_json, second.to_canonical_string().unwrap());
        assert_eq!(first.to_json()["protocol"], EDITOR_STATE_PROTOCOL);
        assert_eq!(first.to_json()["sourceRevision"], 9);
        assert_eq!(first.to_json()["render"]["freshness"], "current");
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
        render.icons.clear();
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
