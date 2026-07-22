//! Reusable host state and artifacts for the model-driven Editor and Play.
//!
//! Rust owns coherent project capture, checking/evaluation, immutable
//! `EditorState`, last-good Play artifacts, and HTTP/SSE transport. The
//! compiled web application owns every browser document and all presentation.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
#[cfg(test)]
use std::path::PathBuf;
use std::path::{Component, Path};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError, sync_channel,
};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

use uhura_base::{
    Diagnostic, FileId, Severity, SourceMap, Span, sha256_hex, to_canonical_json, to_envelope,
    try_to_canonical_json,
};
use uhura_check::assets::{AssetIssue, load_asset_manifest, load_assets};
use uhura_check::icon_fonts::load_icon_fonts;
use uhura_check::project_lock::{
    CapturedPackage, ProjectLockIssue, check_project_lock, parse_project_lock,
};
use uhura_check::project_manifest::{ProjectManifest, ProjectManifestIssue, load_project_manifest};
use uhura_check::resource_manifest::ResourceManifest;
use uhura_check::{
    AssetInput, AuthoringEntryClass as CheckedAuthoringEntryClass, AuthoringProjection,
    AuthoringTarget, AuthoringTargetClass as CheckedAuthoringTargetClass, CheckedIconFonts,
    IconFontInput,
};
use uhura_core::ir::{
    EvidenceRef, Expr, Machine, ScenarioOrigin, SourceRef, Statement, UiAttributeValue, UiNode,
};
use uhura_core::{
    CHECKPOINT_PROTOCOL, Checkpoint, DeploymentContentIdentity, DeploymentIdentityMaterial,
    DeploymentPortBinding, DeploymentPresentationIdentity, EvidenceReport, EvidenceSnapshot,
    Instance, MACHINE_PROGRAM_ID_PROTOCOL, Program, Provenance, ReactionReceipt, Value,
    build_interaction_graph_artifacts, deployment_hash, merge_authored_interaction_topology,
    semantic_node_id,
};
use uhura_editor_model::interaction_graph::{
    EdgeKind as ApplicationEdgeKind, INTERACTION_GRAPH_PROTOCOL, InteractionEdge, InteractionGraph,
    InteractionNode, NodeKind,
};
use uhura_editor_model::{
    Application, Asset, AuthoringMetadata, EditorRender, EditorSourceSpan, EditorState,
    Interaction, InteractionKind, MachineDeployment, MachineSidecar, MachineSidecarInput, Preview,
    PreviewContent, PreviewDocumentation, PreviewEvidence, PreviewField, PreviewFieldGroup,
    PreviewFieldValue, PreviewGroup, PreviewIdentity, PreviewKind, PreviewProvenance,
    RenderFreshness, SourceMetadataClass, SourceMetadataEntry, SourceTarget, SourceTargetClass,
    SourceTargetOwner, SourceTargetOwnerKind, TargetOccurrence, UHURA_EVIDENCE_SUMMARY_PROTOCOL,
    stable_group_id, stable_preview_id,
};

pub mod source;

pub use source::{ProjectSourceFingerprint, ProjectSourceSnapshot, capture_project_snapshot};

const EDITOR_EVENT_PROTOCOL: &str = "uhura-editor-event/0";
const ICON_FONT_MANIFEST_PROTOCOL: &str = "uhura-icon-fonts/0";
const UHURA_INSPECTION_PROTOCOL: &str = "uhura-inspection/1";
const UHURA_PLAY_CONFIG_PROTOCOL: &str = "uhura-play-config/1";
const UHURA_ADAPTER_PROVIDER_PROTOCOL: &str = "uhura-adapter-provider/0";
const UHURA_STYLESHEET_CONTENT_PROTOCOL: &str = "text/css";
const WEB_HISTORY_ADAPTER: &str = "web.history";
const APPLICATION_PROVIDER_ADAPTER: &str = "app.provider";

// ── state and coherent Editor publication ──────────────────────────────────

struct DevState {
    play: PlayState,
    editor: EditorHostState,
}

#[derive(Default)]
struct PlayState {
    generation: u64,
    ok: bool,
    diagnostics: Option<serde_json::Value>,
    /// Last-good artifacts; a rejected generation never replaces these.
    good: Option<GoodBuild>,
}

struct GoodBuild {
    /// Diagnostics belonging to this otherwise usable Play build. This is
    /// `null` when the checker reported nothing and a complete
    /// `uhura-diagnostics/0` envelope when it reported warnings.
    diagnostics: serde_json::Value,
    ir: String,
    inspect_json: String,
    stylesheet: String,
    config_json: String,
    provider_js: Option<String>,
    play_assets: BTreeMap<String, Arc<[u8]>>,
    icon_fonts: Option<IconFontResources>,
}

/// Renderer resources captured from one successful checker result. This is a
/// host-private transport snapshot: semantic artifacts retain only logical
/// icon tokens and never receive font bytes or codepoints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IconFontResources {
    default: String,
    families: BTreeMap<String, IconFontFamilyResource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IconFontFamilyResource {
    font: Arc<[u8]>,
    font_hash: String,
    glyphs: BTreeMap<String, u32>,
}

impl From<&CheckedIconFonts> for IconFontResources {
    fn from(checked: &CheckedIconFonts) -> Self {
        Self {
            default: checked.default.to_string(),
            families: checked
                .families
                .iter()
                .map(|(name, family)| {
                    (
                        name.to_string(),
                        IconFontFamilyResource {
                            font: Arc::clone(&family.font),
                            font_hash: family.font_hash.clone(),
                            glyphs: family
                                .glyphs
                                .iter()
                                .map(|(name, codepoint)| (name.to_string(), *codepoint))
                                .collect(),
                        },
                    )
                })
                .collect(),
        }
    }
}

struct EditorBuildArtifact {
    render: EditorRender,
    icon_fonts: Option<IconFontResources>,
    preview_count: usize,
    replay_derived_count: usize,
    diagnostics: serde_json::Value,
}

struct EditorBuildRejection {
    diagnostics: serde_json::Value,
}

impl EditorBuildRejection {
    fn new(diagnostics: serde_json::Value) -> Self {
        Self { diagnostics }
    }
}

type EditorBuildOutcome = Result<EditorBuildArtifact, EditorBuildRejection>;

/// A complete off-path result for one coherently captured source revision.
/// Hosts may inspect its summary, then atomically publish it into [`Host`].
pub struct ClientCandidate {
    revision: u64,
    source_fingerprint: ProjectSourceFingerprint,
    source_revision_id: String,
    editor: EditorBuildOutcome,
    play: Result<GoodBuild, serde_json::Value>,
    checked_routes: Option<Vec<CheckedRoutePattern>>,
}

/// Build-time facts used by terminal and aggregate-host presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateSummary {
    pub revision: u64,
    pub editor_current: bool,
    pub preview_count: Option<usize>,
    pub replay_derived_count: Option<usize>,
    pub play_ok: bool,
}

/// One route pattern selected by the exact checked deployment behind a
/// candidate.
///
/// Aggregate hosts consume this semantic view for composition policy without
/// reparsing Uhura source or depending on Uhura's private IR encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedRoutePattern {
    table: String,
    constructor: String,
    pattern: String,
    path: CheckedRoutePath,
}

impl CheckedRoutePattern {
    #[must_use]
    pub fn table(&self) -> &str {
        &self.table
    }

    #[must_use]
    pub fn constructor(&self) -> &str {
        &self.constructor
    }

    #[must_use]
    pub fn display_pattern(&self) -> &str {
        &self.pattern
    }

    /// Whether this checked route can produce a path in a host-owned claim.
    ///
    /// The route's source spelling remains diagnostic text only. Uhura owns
    /// the checked path shape and the claim's single optional decode layer, so
    /// aggregate hosts never need to parse or decode Uhura syntax.
    #[must_use]
    pub fn overlaps(&self, claim: RoutePathClaim<'_>) -> bool {
        self.path.overlaps(claim)
    }
}

/// The path representation used by a host before it decides route ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePathDecode {
    /// Compare the origin-form pathname exactly as delivered by the server.
    Raw,
    /// Percent-decode the pathname exactly once before comparing ownership.
    PercentDecodedOnce,
}

/// The topology occupied by one host-owned path claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePathScope {
    /// Only the named path.
    Exact,
    /// The named path and all of its descendants.
    Namespace,
    /// Descendants of the named path, excluding the path itself.
    Descendants,
    /// Every path whose pathname text begins with the named prefix.
    Prefix,
}

/// A host-owned path topology evaluated against a checked Uhura route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutePathClaim<'a> {
    pub path: &'a str,
    pub scope: RoutePathScope,
    pub decode: RoutePathDecode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckedRoutePath {
    raw: Vec<CheckedRoutePathPart>,
    decoded_once: Vec<CheckedRoutePathPart>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CheckedRoutePathPart {
    Literal(String),
    Dynamic,
}

impl CheckedRoutePath {
    fn from_checked_parts(parts: &[uhura_port::RoutePathPart]) -> Self {
        let raw = parts
            .iter()
            .map(|part| match part {
                uhura_port::RoutePathPart::Literal(literal) => {
                    CheckedRoutePathPart::Literal(literal.clone())
                }
                uhura_port::RoutePathPart::Field(_) => CheckedRoutePathPart::Dynamic,
            })
            .collect();
        let decoded_once = parts
            .iter()
            .flat_map(|part| match part {
                uhura_port::RoutePathPart::Field(_) => {
                    vec![CheckedRoutePathPart::Dynamic]
                }
                uhura_port::RoutePathPart::Literal(literal) => {
                    let decoded = uhura_port::decode_query_value(literal)
                        .expect("checked literal route components are canonical");
                    decoded
                        .split('/')
                        .map(|segment| CheckedRoutePathPart::Literal(segment.to_string()))
                        .collect()
                }
            })
            .collect();
        Self { raw, decoded_once }
    }

    fn overlaps(&self, claim: RoutePathClaim<'_>) -> bool {
        let Some(claim_segments) = route_claim_segments(claim.path) else {
            return false;
        };
        let (parts, dynamic_expands) = match claim.decode {
            RoutePathDecode::Raw => (self.raw.as_slice(), false),
            RoutePathDecode::PercentDecodedOnce => (self.decoded_once.as_slice(), true),
        };
        match claim.scope {
            RoutePathScope::Exact => {
                route_path_matches_exact(parts, &claim_segments, dynamic_expands)
            }
            RoutePathScope::Namespace => {
                route_path_matches_exact(parts, &claim_segments, dynamic_expands)
                    || route_path_matches_descendant(parts, &claim_segments, dynamic_expands)
            }
            RoutePathScope::Descendants => {
                route_path_matches_descendant(parts, &claim_segments, dynamic_expands)
            }
            RoutePathScope::Prefix => route_path_matches_prefix(parts, claim.path, dynamic_expands),
        }
    }
}

fn route_claim_segments(path: &str) -> Option<Vec<&str>> {
    let relative = path.strip_prefix('/')?;
    if relative.is_empty() {
        Some(Vec::new())
    } else {
        Some(relative.split('/').collect())
    }
}

fn route_path_matches_exact(
    parts: &[CheckedRoutePathPart],
    target: &[&str],
    dynamic_expands: bool,
) -> bool {
    if parts.is_empty() {
        return target.is_empty();
    }
    let Some((part, rest)) = parts.split_first() else {
        return target.is_empty();
    };
    match part {
        CheckedRoutePathPart::Literal(literal) => {
            target.first().is_some_and(|segment| *segment == literal)
                && route_path_matches_exact(rest, &target[1..], dynamic_expands)
        }
        CheckedRoutePathPart::Dynamic if !dynamic_expands => {
            !target.is_empty() && route_path_matches_exact(rest, &target[1..], dynamic_expands)
        }
        CheckedRoutePathPart::Dynamic => (1..=target.len())
            .any(|consumed| route_path_matches_exact(rest, &target[consumed..], dynamic_expands)),
    }
}

fn route_path_matches_descendant(
    parts: &[CheckedRoutePathPart],
    namespace: &[&str],
    dynamic_expands: bool,
) -> bool {
    if namespace.is_empty() {
        return !parts.is_empty();
    }
    let Some((part, rest)) = parts.split_first() else {
        return false;
    };
    match part {
        CheckedRoutePathPart::Literal(literal) => {
            namespace.first().is_some_and(|segment| *segment == literal)
                && route_path_matches_descendant(rest, &namespace[1..], dynamic_expands)
        }
        CheckedRoutePathPart::Dynamic if !dynamic_expands => {
            route_path_matches_descendant(rest, &namespace[1..], dynamic_expands)
        }
        // One decoded dynamic component may existentially encode every
        // remaining namespace segment plus at least one descendant segment.
        CheckedRoutePathPart::Dynamic => true,
    }
}

fn route_path_matches_prefix(
    parts: &[CheckedRoutePathPart],
    prefix: &str,
    dynamic_expands: bool,
) -> bool {
    let Some(relative) = prefix.strip_prefix('/') else {
        return false;
    };
    if relative.is_empty() {
        return true;
    }
    let segments = relative.split('/').collect::<Vec<_>>();
    let (segment_prefix, exact_prefix) = segments
        .split_last()
        .expect("a non-empty relative prefix has one segment");
    route_path_matches_segment_prefix(parts, exact_prefix, segment_prefix, dynamic_expands)
}

fn route_path_matches_segment_prefix(
    parts: &[CheckedRoutePathPart],
    exact_prefix: &[&str],
    segment_prefix: &str,
    dynamic_expands: bool,
) -> bool {
    let Some((part, rest)) = parts.split_first() else {
        return false;
    };
    if exact_prefix.is_empty() {
        return match part {
            CheckedRoutePathPart::Literal(literal) => literal.starts_with(segment_prefix),
            CheckedRoutePathPart::Dynamic => true,
        };
    }
    match part {
        CheckedRoutePathPart::Literal(literal) => {
            literal == exact_prefix[0]
                && route_path_matches_segment_prefix(
                    rest,
                    &exact_prefix[1..],
                    segment_prefix,
                    dynamic_expands,
                )
        }
        CheckedRoutePathPart::Dynamic if !dynamic_expands => route_path_matches_segment_prefix(
            rest,
            &exact_prefix[1..],
            segment_prefix,
            dynamic_expands,
        ),
        // The one decoded component can encode the remaining exact segments
        // and the segment beginning with the claimed text prefix.
        CheckedRoutePathPart::Dynamic => true,
    }
}

/// One aggregate-host deployment rejection attached to an otherwise checked
/// candidate before publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayAdmissionRejection {
    code: String,
    rule: String,
    message: String,
}

impl PlayAdmissionRejection {
    pub fn new(
        code: impl Into<String>,
        rule: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            rule: rule.into(),
            message: message.into(),
        }
    }
}

/// Structured diagnostics produced while building one client candidate.
///
/// Each component is either JSON `null` or a complete
/// `uhura-diagnostics/0` envelope. A checked Editor may remain current
/// while carrying evidence or deployment-admission errors because its graph
/// and static projections do not depend on an admitted Play target. Play
/// diagnostics independently describe whether the live target was admitted.
/// Use [`ClientCandidate::summary`] to distinguish acceptance from rejection.
/// This view borrows the off-path candidate and does not construct a [`Host`]
/// or load [`WebAssets`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateDiagnostics<'a> {
    pub editor: &'a serde_json::Value,
    pub play: &'a serde_json::Value,
}

impl ClientCandidate {
    pub fn summary(&self) -> CandidateSummary {
        let editor = self.editor.as_ref().ok();
        CandidateSummary {
            revision: self.revision,
            editor_current: editor.is_some(),
            preview_count: editor.map(EditorBuildArtifact::preview_count),
            replay_derived_count: editor.map(EditorBuildArtifact::replay_derived_count),
            play_ok: self.play.is_ok(),
        }
    }

    /// Inspect the exact Editor and Play diagnostics produced for this
    /// candidate without publishing it.
    #[must_use]
    pub fn diagnostics(&self) -> CandidateDiagnostics<'_> {
        let editor = match &self.editor {
            Ok(artifact) => artifact.diagnostics(),
            Err(rejection) => &rejection.diagnostics,
        };
        let play = match &self.play {
            Ok(artifact) => &artifact.diagnostics,
            Err(diagnostics) => diagnostics,
        };
        CandidateDiagnostics { editor, play }
    }

    /// Checked application route patterns for aggregate-host admission.
    ///
    /// `None` means no checked selected `web.history` deployment could be
    /// resolved. A candidate retains this view when route ownership rejects
    /// that otherwise valid deployment, allowing aggregate hosts to compose
    /// diagnostics without source or private-IR access.
    #[must_use]
    pub fn checked_route_patterns(&self) -> Option<&[CheckedRoutePattern]> {
        self.checked_routes.as_deref()
    }

    /// Reject this candidate's Play artifact at an aggregate-host admission
    /// boundary while retaining its current Editor graph and checked semantic
    /// views.
    ///
    /// Callers must attach every composition rejection before passing the
    /// candidate to [`Host::new`] or [`Host::publish`]. Publishing then updates
    /// Editor diagnostics and preserves any last-good Play artifact atomically.
    pub fn reject_play_admission(&mut self, rejection: PlayAdmissionRejection) {
        let envelope = host_failure(&rejection.code, &rejection.rule, rejection.message);
        if let Ok(editor) = &mut self.editor {
            editor.diagnostics = merge_diagnostics([editor.diagnostics.clone(), envelope.clone()]);
        }
        let previous = self
            .play
            .as_ref()
            .err()
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        self.play = Err(merge_diagnostics([previous, envelope]));
    }

    /// Content identity of the exact coherent source snapshot consumed by
    /// this candidate build.
    ///
    /// This source identity, rather than an artifact-output hash, is the v1
    /// candidate identity. Editor artifacts embed the publication revision,
    /// while all Editor and Play artifact encodings remain private,
    /// toolchain-derived implementation details. Hashing those outputs would
    /// either make an unchanged source acquire a new identity at every
    /// publication or promise cross-toolchain stability the host does not
    /// provide. Within one Uhura binary the artifacts are deterministic
    /// functions of this captured source and the requested revision.
    #[must_use]
    pub fn source_fingerprint(&self) -> &ProjectSourceFingerprint {
        &self.source_fingerprint
    }

    /// Canonical `uhura-source-revision/0` identity for the captured paths
    /// and raw bytes.
    #[must_use]
    pub fn source_id(&self) -> String {
        self.source_revision_id.clone()
    }
}

impl EditorBuildArtifact {
    fn preview_count(&self) -> usize {
        self.preview_count
    }

    fn replay_derived_count(&self) -> usize {
        self.replay_derived_count
    }

    fn diagnostics(&self) -> &serde_json::Value {
        &self.diagnostics
    }

    fn into_publication(self) -> (EditorRender, Option<IconFontResources>, serde_json::Value) {
        (self.render, self.icon_fonts, self.diagnostics)
    }
}

/// State that became visible after one atomic publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationReport {
    pub source_revision: u64,
    pub editor_current: bool,
    pub preview_count: Option<usize>,
    pub replay_derived_count: Option<usize>,
    pub play_generation: u64,
    pub play_ok: bool,
    pub has_good_play: bool,
}

struct EditorHostState {
    source_revision: u64,
    state_json: String,
    /// Always kept with its original render revision and `current` marker;
    /// stale publication mutates a clone only.
    last_renderable: Option<EditorRenderable>,
}

#[derive(Clone)]
struct EditorRenderable {
    render: Box<EditorRender>,
    icon_fonts: Option<IconFontResources>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RevisionOrderError {
    expected: u64,
    received: u64,
}

impl std::fmt::Display for RevisionOrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Editor candidate revision {} arrived; expected {}",
            self.received, self.expected
        )
    }
}

impl std::error::Error for RevisionOrderError {}

impl EditorHostState {
    fn initial(outcome: EditorBuildOutcome) -> Result<Self, String> {
        let (state_json, last_renderable) = materialize_editor_state(1, outcome, None)?;
        Ok(Self {
            source_revision: 1,
            state_json,
            last_renderable,
        })
    }

    fn apply(&mut self, revision: u64, outcome: EditorBuildOutcome) -> Result<(), String> {
        let expected = self.source_revision + 1;
        if revision != expected {
            return Err(RevisionOrderError {
                expected,
                received: revision,
            }
            .to_string());
        }
        // Build the whole replacement before mutating the published slot.
        let (state_json, last_renderable) =
            materialize_editor_state(revision, outcome, self.last_renderable.as_ref())?;
        self.source_revision = revision;
        self.state_json = state_json;
        self.last_renderable = last_renderable;
        Ok(())
    }
}

fn materialize_editor_state(
    revision: u64,
    outcome: EditorBuildOutcome,
    last_renderable: Option<&EditorRenderable>,
) -> Result<(String, Option<EditorRenderable>), String> {
    match outcome {
        Ok(artifact) => {
            let (render, icon_fonts, diagnostics) = artifact.into_publication();
            let next_renderable = EditorRenderable {
                render: Box::new(render.clone()),
                icon_fonts,
            };
            let state = EditorState::current(revision, diagnostics, render)
                .map_err(|error| error.to_string())?;
            let state_json = state
                .to_canonical_string()
                .map_err(|error| error.to_string())?;
            Ok((state_json, Some(next_renderable)))
        }
        Err(rejection) => match last_renderable {
            Some(EditorRenderable { render, .. }) => {
                let state =
                    EditorState::stale(revision, rejection.diagnostics, render.as_ref().clone())
                        .map_err(|error| error.to_string())?;
                let state_json = state
                    .to_canonical_string()
                    .map_err(|error| error.to_string())?;
                Ok((state_json, last_renderable.cloned()))
            }
            None => {
                let state = EditorState::cold_invalid(revision, rejection.diagnostics)
                    .map_err(|error| error.to_string())?;
                let state_json = state
                    .to_canonical_string()
                    .map_err(|error| error.to_string())?;
                Ok((state_json, None))
            }
        },
    }
}

/// Build Editor and Play from exactly the bytes in `snapshot`.
pub fn build_candidate(snapshot: &ProjectSourceSnapshot, revision: u64) -> ClientCandidate {
    let (editor, play, checked_routes) = build_machine_candidate(snapshot, revision);
    ClientCandidate {
        revision,
        source_fingerprint: snapshot.fingerprint.clone(),
        source_revision_id: snapshot.source_revision_id().to_string(),
        editor,
        play,
        checked_routes,
    }
}

fn apply_play(play: &mut PlayState, outcome: Result<GoodBuild, serde_json::Value>) {
    play.generation += 1;
    match outcome {
        Ok(good) => {
            play.ok = true;
            play.diagnostics = None;
            play.good = Some(good);
        }
        Err(envelope) => {
            play.ok = false;
            play.diagnostics = Some(envelope);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Deployment {
    entry: String,
    machine: String,
    presentation: Option<String>,
    lifetime: String,
    /// Exact canonical tagged Uhura JSON transported by the closed host
    /// manifest. `None` is valid only for a Unit-configured machine.
    configuration: Option<String>,
    ports: BTreeMap<String, String>,
    stylesheet: Option<String>,
    provider: Option<ApplicationProvider>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApplicationProvider {
    module: String,
    config: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeploymentAdmission {
    configuration: Value,
    ports: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeploymentIdentity {
    protocol: String,
    machine_program_hash: String,
    presentation_hash: Option<String>,
    evidence_hash: Option<String>,
    deployment_hash: String,
}

struct EditorInputs<'a> {
    evidence: &'a EvidenceReport,
    sources: &'a [(String, String)],
    provenance: &'a [serde_json::Value],
    semantic_provenance: &'a Provenance,
    interaction_graph: &'a serde_json::Value,
    graph_sources: &'a serde_json::Value,
    authoring: &'a AuthoringProjection,
    assets: &'a BTreeMap<String, Asset>,
}

struct CheckedProject {
    program: Program,
    /// Host-selector roots resolved from the same project manifest as the
    /// checked program.
    selector_packages: BTreeMap<String, String>,
    icon_fonts: CheckedIconFonts,
    editor_assets: BTreeMap<String, Asset>,
    play_assets: BTreeMap<String, Arc<[u8]>>,
    diagnostics: serde_json::Value,
    evidence: EvidenceReport,
    evidence_diagnostics: serde_json::Value,
    sources: Vec<(String, String)>,
    provenance: Vec<serde_json::Value>,
    semantic_provenance: Provenance,
    interaction_graph: serde_json::Value,
    graph_sources: serde_json::Value,
    authoring: AuthoringProjection,
}

struct ProjectResources {
    icon_fonts: CheckedIconFonts,
    editor_assets: BTreeMap<String, Asset>,
    play_assets: BTreeMap<String, Arc<[u8]>>,
}

struct PlayAuthority {
    deployment: Deployment,
    identity: DeploymentIdentity,
    configuration: Value,
    admitted_ports: Vec<serde_json::Value>,
    stylesheet: String,
    provider_js: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EvidenceSummary {
    passed: bool,
    scenarios: EvidenceScenarioCounts,
    artifacts: EvidenceArtifactCounts,
    failure_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EvidenceScenarioCounts {
    total: usize,
    passed: usize,
    failed: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EvidenceArtifactCounts {
    pins: usize,
    examples: usize,
    checkpoints: usize,
}

impl EvidenceSummary {
    fn from_report(report: &EvidenceReport) -> Self {
        let passed = report
            .scenarios
            .iter()
            .filter(|scenario| scenario.status == uhura_core::ScenarioStatus::Passed)
            .count();
        let failed = report.scenarios.len() - passed;
        Self {
            passed: report.passed,
            scenarios: EvidenceScenarioCounts {
                total: report.scenarios.len(),
                passed,
                failed,
            },
            artifacts: EvidenceArtifactCounts {
                pins: report.artifacts.pins.len(),
                examples: report.artifacts.examples.len(),
                checkpoints: report.artifacts.checkpoints.len(),
            },
            failure_count: report.failures.len(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "protocol": UHURA_EVIDENCE_SUMMARY_PROTOCOL,
            "passed": self.passed,
            "scenarios": {
                "total": self.scenarios.total,
                "passed": self.scenarios.passed,
                "failed": self.scenarios.failed,
            },
            "artifacts": {
                "pins": self.artifacts.pins,
                "examples": self.artifacts.examples,
                "checkpoints": self.artifacts.checkpoints,
            },
            "failureCount": self.failure_count,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostAdapter {
    /// The browser delegates route encoding and decoding to the checked
    /// Router instance in Wasm, so it supports every valid pinned Router
    /// instantiation rather than one ambient route table.
    WebHistory,
    /// An explicitly configured application module implements the exact
    /// checked port instance outside Uhura Core. Contract hashes are still
    /// admitted here and rechecked by the browser adapter host.
    ApplicationProvider,
}

fn build_machine_candidate(
    snapshot: &ProjectSourceSnapshot,
    revision: u64,
) -> (
    EditorBuildOutcome,
    Result<GoodBuild, serde_json::Value>,
    Option<Vec<CheckedRoutePattern>>,
) {
    let checked = match check_project_snapshot(snapshot) {
        Ok(checked) => checked,
        Err(diagnostics) => {
            return (
                Err(EditorBuildRejection::new(diagnostics.clone())),
                Err(diagnostics),
                None,
            );
        }
    };
    let authority = admit_play(snapshot, &checked.program, &checked.selector_packages);
    let checked_routes = authority
        .as_ref()
        .ok()
        .map(|authority| checked_route_patterns(&checked.program, &authority.deployment));
    let authority = authority.and_then(|authority| {
        if let Some((route, reserved)) =
            standalone_web_host_route_collision(checked_routes.as_deref().unwrap_or_default())
        {
            return Err(host_failure(
                "R3014",
                "uhura/reserved-web-host-route",
                format!(
                    "route `{}` pattern `{}` can encode standalone-host path `{reserved}`; the checked graph remains valid, but this deployment cannot route that path to the machine",
                    route.constructor(),
                    route.display_pattern(),
                ),
            ));
        }
        Ok(authority)
    });
    let admission_diagnostics = authority.as_ref().err().cloned();
    let editor_diagnostics = merge_diagnostics([
        checked.diagnostics.clone(),
        checked.evidence_diagnostics.clone(),
        admission_diagnostics.unwrap_or(serde_json::Value::Null),
    ]);
    let editor = build_editor(
        revision,
        &checked,
        authority.as_ref().ok(),
        editor_diagnostics,
    )
    .map_err(EditorBuildRejection::new);
    let play = authority.map(|authority| build_play(&checked, authority));
    (editor, play, checked_routes)
}

fn check_project_snapshot(
    snapshot: &ProjectSourceSnapshot,
) -> Result<CheckedProject, serde_json::Value> {
    snapshot.validate_for_build()?;
    let manifest_text = snapshot
        .files
        .text("uhura.toml")
        .map_err(|message| {
            host_failure(
                "UH2001",
                "contract/invalid-manifest",
                format!("uhura.toml: {message}"),
            )
        })?
        .unwrap_or_default();
    let manifest = load_project_manifest(&manifest_text).map_err(project_manifest_diagnostics)?;
    let sources = snapshot
        .files
        .sources()
        .map_err(|message| host_failure("R3014", "uhura/source", message))?;
    let resources = load_project_resources(snapshot, &manifest.resources)?;
    let selector_packages = std::iter::once((
        "crate".to_string(),
        manifest.project.package_id().to_string(),
    ))
    .chain(manifest.dependencies.iter().map(|(alias, dependency)| {
        (
            alias.as_str().to_string(),
            dependency.package_id().to_string(),
        )
    }))
    .collect();

    let mut source_map = SourceMap::new();
    for (path, text) in &sources {
        source_map.add(path.clone(), text.clone());
    }
    let uhura_check::CheckOutput {
        mut diagnostics,
        program,
        provenance: semantic_provenance,
        authoring,
    } = check_sources(snapshot, &sources, &manifest)?;
    if let Some(program) = program.as_ref() {
        diagnostics.extend(uhura_check::icon_token_diagnostics(
            program,
            &resources.icon_fonts,
            sources
                .iter()
                .enumerate()
                .map(|(file, (path, _))| (FileId(file as u32), path.as_str())),
        ));
    }
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
        )
    });
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(to_envelope(&diagnostics, &source_map));
    }
    let diagnostics_json = if diagnostics.is_empty() {
        serde_json::Value::Null
    } else {
        to_envelope(&diagnostics, &source_map)
    };
    let mut program = program.ok_or_else(|| {
        host_failure(
            "R3014",
            "uhura/lowering",
            "the Uhura checker accepted the project but produced no program",
        )
    })?;
    let semantic_provenance = require_semantic_provenance(semantic_provenance)?;
    semantic_provenance.validate().map_err(|error| {
        host_failure(
            "R3014",
            "uhura/provenance",
            format!("checked provenance is invalid: {error}"),
        )
    })?;
    program.freeze_program_hashes();

    let evidence = program.run_evidence();
    let evidence_diagnostics = if evidence.passed {
        serde_json::Value::Null
    } else {
        evidence_failure(&evidence, &source_map, &sources)
    };
    let provenance = source_provenance(&sources);
    // Build topology once. Editor and Play inspection publish byte-equivalent
    // semantic and physical-source read models from this same checked program.
    let mut interaction = build_interaction_graph_artifacts(&program);
    merge_authored_interaction_topology(&mut interaction, &semantic_provenance).map_err(
        |error| {
            host_failure(
                "R3014",
                "uhura/interaction-topology",
                format!("checked authored topology could not be projected: {error}"),
            )
        },
    )?;
    validate_interaction_graph_source_inventory(&interaction, &sources).map_err(|message| {
        host_failure(
            "R3014",
            "uhura/interaction-topology",
            format!("checked interaction graph source inventory is invalid: {message}"),
        )
    })?;
    let interaction_graph =
        serde_json::to_value(interaction.graph).expect("Uhura interaction graph is serializable");
    let graph_sources = serde_json::to_value(interaction.provenance)
        .expect("Uhura interaction graph provenance is serializable");
    Ok(CheckedProject {
        program,
        selector_packages,
        icon_fonts: resources.icon_fonts,
        editor_assets: resources.editor_assets,
        play_assets: resources.play_assets,
        diagnostics: diagnostics_json,
        evidence,
        evidence_diagnostics,
        sources,
        provenance,
        semantic_provenance,
        interaction_graph,
        graph_sources,
        authoring,
    })
}

fn require_semantic_provenance(
    provenance: Option<Provenance>,
) -> Result<Provenance, serde_json::Value> {
    provenance.ok_or_else(|| {
        host_failure(
            "R3014",
            "uhura/provenance",
            "the Uhura checker accepted the project but produced no semantic provenance",
        )
    })
}

fn validate_interaction_graph_source_inventory(
    artifacts: &uhura_core::InteractionGraphArtifacts,
    sources: &[(String, String)],
) -> Result<(), String> {
    let inventory = sources
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect::<BTreeMap<_, _>>();
    let graph_nodes = artifacts
        .graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let sourced_nodes = artifacts
        .provenance
        .nodes
        .iter()
        .map(|entry| entry.node.as_str())
        .collect::<BTreeSet<_>>();
    if graph_nodes != sourced_nodes || graph_nodes.len() != artifacts.provenance.nodes.len() {
        return Err("node provenance must cover every graph node exactly once".into());
    }
    let graph_edges = artifacts.graph.edges.iter().collect::<BTreeSet<_>>();
    let sourced_edges = artifacts
        .provenance
        .edges
        .iter()
        .map(|entry| &entry.edge)
        .collect::<BTreeSet<_>>();
    if graph_edges != sourced_edges || graph_edges.len() != artifacts.provenance.edges.len() {
        return Err("edge provenance must cover every graph edge exactly once".into());
    }

    let validate_sources = |owner: &str, sources: &[SourceRef]| -> Result<(), String> {
        if sources.is_empty() {
            return Err(format!("{owner} has no physical source"));
        }
        for source in sources {
            let text = inventory.get(source.path.as_str()).ok_or_else(|| {
                format!(
                    "graph source `{}` does not occur in the accepted project",
                    source.path
                )
            })?;
            let start = usize::try_from(source.start)
                .map_err(|_| "graph source start does not fit usize".to_string())?;
            let end = usize::try_from(source.end)
                .map_err(|_| "graph source end does not fit usize".to_string())?;
            if start > end
                || end > text.len()
                || !text.is_char_boundary(start)
                || !text.is_char_boundary(end)
            {
                return Err(format!(
                    "graph source `{}` range {}..{} is outside its accepted UTF-8 bytes",
                    source.path, source.start, source.end
                ));
            }
        }
        Ok(())
    };
    for entry in &artifacts.provenance.nodes {
        validate_sources(&format!("graph node `{}`", entry.node), &entry.sources)?;
    }
    for entry in &artifacts.provenance.edges {
        validate_sources(
            &format!("graph edge `{} -> {}`", entry.edge.from, entry.edge.to),
            &entry.sources,
        )?;
    }
    Ok(())
}

fn check_sources(
    snapshot: &ProjectSourceSnapshot,
    sources: &[(String, String)],
    manifest: &ProjectManifest,
) -> Result<uhura_check::CheckOutput, serde_json::Value> {
    let captured_dependencies = capture_dependencies(snapshot, sources, manifest)?;
    let dependency_roots = captured_dependencies
        .iter()
        .map(|package| package.source.as_str())
        .collect::<Vec<_>>();
    let messages = validate_source_inventory(snapshot, sources, manifest, &dependency_roots);
    if !messages.is_empty() {
        return Err(diagnostics_envelope(
            "UH2001",
            "contract/invalid-project",
            messages,
        ));
    }

    let compiler_sources = sources
        .iter()
        .enumerate()
        .map(|(file, (path, text))| {
            uhura_check::ProjectSource::new(FileId(file as u32), path, text)
        })
        .collect::<Vec<_>>();
    Ok(uhura_check::compile_project(
        manifest,
        &compiler_sources,
        &captured_dependencies,
    ))
}

fn capture_dependencies(
    snapshot: &ProjectSourceSnapshot,
    sources: &[(String, String)],
    manifest: &ProjectManifest,
) -> Result<Vec<CapturedPackage>, serde_json::Value> {
    let lock_text = snapshot.files.text("uhura.lock").map_err(|message| {
        diagnostics_envelope("UH2001", "contract/invalid-project", vec![message])
    })?;
    if manifest.dependencies.is_empty() {
        return check_project_lock(manifest, lock_text.as_deref(), &[])
            .map(|_| Vec::new())
            .map_err(|issues| {
                diagnostics_envelope(
                    "UH2001",
                    "contract/invalid-project",
                    lock_issue_messages(issues),
                )
            });
    }
    let lock = parse_project_lock(lock_text.as_deref().ok_or_else(|| {
        diagnostics_envelope(
            "UH2001",
            "contract/invalid-project",
            vec!["uhura.lock: lock file is required".into()],
        )
    })?)
    .map_err(|issues| {
        diagnostics_envelope(
            "UH2001",
            "contract/invalid-project",
            lock_issue_messages(issues),
        )
    })?;
    let mut captured = Vec::new();
    let mut messages = Vec::new();
    let dependency_roots = lock
        .packages
        .values()
        .map(|record| record.source.path.as_str())
        .collect::<Vec<_>>();
    for record in lock.packages.values() {
        let manifest_path = format!("{}/uhura.toml", record.source.path);
        let manifest_text = match snapshot.files.text(&manifest_path) {
            Ok(Some(text)) => text,
            Ok(None) => {
                messages.push(format!(
                    "package.{}.manifest: `{manifest_path}` is missing",
                    record.package
                ));
                continue;
            }
            Err(error) => {
                messages.push(format!("package.{}.manifest: {error}", record.package));
                continue;
            }
        };
        let package_manifest = match load_project_manifest(&manifest_text) {
            Ok(manifest) => manifest,
            Err(issues) => {
                messages.extend(issues.into_iter().map(|issue| {
                    format!(
                        "package.{}.manifest.{}: {}",
                        record.package, issue.path, issue.message
                    )
                }));
                continue;
            }
        };
        let declared_sources = package_manifest
            .modules
            .values()
            .chain(package_manifest.evidence.values())
            .map(|path| format!("{}/{}", record.source.path, path))
            .collect::<BTreeSet<_>>();
        let discovered_sources = sources
            .iter()
            .filter(|(path, _)| {
                owning_dependency_root(path, &dependency_roots) == Some(record.source.path.as_str())
            })
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>();
        for unlisted in discovered_sources.difference(&declared_sources) {
            messages.push(format!(
                "package.{}.sources: `{unlisted}` is not listed in `[modules]` or `[evidence.modules]`",
                record.package
            ));
        }
        let mut module_bytes = BTreeMap::new();
        for (logical, physical) in &package_manifest.modules {
            let global = format!("{}/{}", record.source.path, physical);
            let Some((_, source)) = sources.iter().find(|(path, _)| path == &global) else {
                messages.push(format!(
                    "package.{}.modules.{}: mapped source `{global}` is missing",
                    record.package, logical
                ));
                continue;
            };
            module_bytes.insert(logical.clone(), source.as_bytes().to_vec());
        }
        let resolved_dependencies = package_manifest
            .dependencies
            .iter()
            .map(|(alias, dependency)| (alias.clone(), dependency.package_id()))
            .collect();
        captured.push(CapturedPackage {
            manifest: package_manifest,
            source: record.source.path.clone(),
            modules: module_bytes,
            resolved_dependencies,
            resources: BTreeMap::new(),
        });
    }
    if !messages.is_empty() {
        messages.sort();
        return Err(diagnostics_envelope(
            "UH2001",
            "contract/invalid-project",
            messages,
        ));
    }
    check_project_lock(manifest, lock_text.as_deref(), &captured).map_err(|issues| {
        diagnostics_envelope(
            "UH2001",
            "contract/invalid-project",
            lock_issue_messages(issues),
        )
    })?;
    Ok(captured)
}

fn lock_issue_messages(issues: Vec<ProjectLockIssue>) -> Vec<String> {
    issues
        .into_iter()
        .map(|issue| {
            if issue.path.is_empty() {
                issue.message
            } else {
                format!("{}: {}", issue.path, issue.message)
            }
        })
        .collect()
}

fn path_is_within(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn owning_dependency_root<'a>(path: &str, roots: &[&'a str]) -> Option<&'a str> {
    roots
        .iter()
        .copied()
        .filter(|root| path_is_within(path, root))
        .max_by_key(|root| root.len())
}

fn validate_source_inventory(
    snapshot: &ProjectSourceSnapshot,
    sources: &[(String, String)],
    manifest: &ProjectManifest,
    dependency_roots: &[&str],
) -> Vec<String> {
    let mut messages = Vec::new();
    let declared = manifest
        .modules
        .values()
        .chain(manifest.evidence.values())
        .map(|path| path.as_str())
        .collect::<BTreeSet<_>>();
    let discovered = sources
        .iter()
        .filter(|(path, _)| {
            !dependency_roots
                .iter()
                .any(|root| path_is_within(path, root))
        })
        .map(|(path, _)| path.as_str())
        .collect::<BTreeSet<_>>();
    for missing in declared.difference(&discovered) {
        messages.push(format!(
            "mapped Uhura 0.4 source `{missing}` is missing from the project"
        ));
    }
    for unlisted in discovered.difference(&declared) {
        messages.push(format!(
            "Uhura 0.4 source `{unlisted}` is not listed in `[modules]` or `[evidence.modules]`"
        ));
    }
    for aliases in snapshot.files.duplicate_sources() {
        messages.push(format!(
            "Uhura source paths {} resolve to the same physical file",
            aliases
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    messages.sort();
    messages
}

fn load_project_resources(
    snapshot: &ProjectSourceSnapshot,
    manifest: &ResourceManifest,
) -> Result<ProjectResources, serde_json::Value> {
    let icon_fonts = load_project_icon_fonts(snapshot, manifest)?;
    let (editor_assets, play_assets) =
        load_project_assets(snapshot, manifest.assets.manifest.as_deref())?;
    Ok(ProjectResources {
        icon_fonts,
        editor_assets,
        play_assets,
    })
}

fn load_project_icon_fonts(
    snapshot: &ProjectSourceSnapshot,
    manifest: &ResourceManifest,
) -> Result<CheckedIconFonts, serde_json::Value> {
    let mut inputs = BTreeMap::new();
    for (name, family) in &manifest.icons.families {
        let font_bytes = snapshot
            .files
            .resolve(Path::new(&family.font))
            .map_err(|message| {
                host_failure(
                    "UH2010",
                    "contract/invalid-icon-font",
                    format!("icons.{name}.font: {message}"),
                )
            })?
            .map(Arc::clone);
        let glyphs_text = snapshot.files.text(&family.glyphs).map_err(|message| {
            host_failure(
                "UH2010",
                "contract/invalid-icon-font",
                format!("icons.{name}.glyphs: {message}"),
            )
        })?;
        inputs.insert(
            name.clone(),
            IconFontInput {
                font_path: family.font.clone(),
                font_bytes,
                glyphs_path: family.glyphs.clone(),
                glyphs_text,
            },
        );
    }

    load_icon_fonts(&manifest.icons, &inputs).map_err(|issues| {
        diagnostics_envelope(
            "UH2010",
            "contract/invalid-icon-font",
            issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.path, issue.message)),
        )
    })
}

type LoadedProjectAssets = (BTreeMap<String, Asset>, BTreeMap<String, Arc<[u8]>>);

fn load_project_assets(
    snapshot: &ProjectSourceSnapshot,
    manifest_relative: Option<&str>,
) -> Result<LoadedProjectAssets, serde_json::Value> {
    let Some(manifest_relative) = manifest_relative else {
        return Ok((
            BTreeMap::new(),
            snapshot.files.subtree(Path::new("fixtures/assets")),
        ));
    };
    let manifest_path = Path::new(manifest_relative);
    let manifest_text = snapshot
        .files
        .text(manifest_relative)
        .map_err(|message| {
            host_failure(
                "UH2001",
                "contract/invalid-manifest",
                format!("{manifest_relative}: {message}"),
            )
        })?
        .ok_or_else(|| {
            host_failure(
                "UH2001",
                "contract/invalid-manifest",
                format!("{manifest_relative}: asset manifest is missing"),
            )
        })?;
    let manifest = load_asset_manifest(&manifest_text)
        .map_err(|issues| asset_diagnostics(manifest_relative, issues))?;
    let asset_dir = manifest_path.parent().unwrap_or(Path::new(""));
    let mut inputs = BTreeMap::new();
    for (id, declaration) in &manifest.assets {
        let path = source::normalize_project_path(&asset_dir.join(&declaration.file)).map_err(
            |message| {
                host_failure(
                    "UH2001",
                    "contract/invalid-manifest",
                    format!("{manifest_relative} assets.{id}.file: {message}"),
                )
            },
        )?;
        let bytes = snapshot
            .files
            .resolve(&path)
            .map_err(|message| {
                host_failure(
                    "UH2001",
                    "contract/invalid-manifest",
                    format!("{manifest_relative} assets.{id}.file: {message}"),
                )
            })?
            .map(Arc::clone);
        inputs.insert(
            id.clone(),
            AssetInput {
                file: declaration.file.clone(),
                bytes,
            },
        );
    }
    let checked = load_assets(&manifest, &inputs)
        .map_err(|issues| asset_diagnostics(manifest_relative, issues))?;
    let editor_assets = checked
        .assets
        .iter()
        .map(|(id, asset)| {
            (
                id.to_string(),
                Asset {
                    data_uri: format!("data:{};base64,{}", asset.media_type, base64(&asset.bytes)),
                    alt: asset.alt.clone(),
                },
            )
        })
        .collect();
    Ok((editor_assets, snapshot.files.subtree(asset_dir)))
}

fn asset_diagnostics(manifest: &str, issues: Vec<AssetIssue>) -> serde_json::Value {
    diagnostics_envelope(
        "UH2001",
        "contract/invalid-manifest",
        issues.into_iter().map(|issue| {
            if issue.path.is_empty() {
                format!("{manifest}: {}", issue.message)
            } else {
                format!("{manifest} {}: {}", issue.path, issue.message)
            }
        }),
    )
}

fn project_manifest_diagnostics(issues: Vec<ProjectManifestIssue>) -> serde_json::Value {
    diagnostics_envelope(
        "UH2001",
        "contract/invalid-manifest",
        issues.into_iter().map(|issue| {
            if issue.path.is_empty() {
                format!("uhura.toml: {}", issue.message)
            } else {
                format!("uhura.toml {}: {}", issue.path, issue.message)
            }
        }),
    )
}

fn diagnostics_envelope(
    code: &str,
    rule: &str,
    messages: impl IntoIterator<Item = String>,
) -> serde_json::Value {
    let diagnostics = messages
        .into_iter()
        .map(|message| {
            serde_json::json!({
                "code": code,
                "rule": rule,
                "severity": "error",
                "message": message,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "format": "uhura-diagnostics",
        "version": 0,
        "summary": { "errors": diagnostics.len(), "warnings": 0 },
        "diagnostics": diagnostics,
    })
}

const STANDALONE_WEB_HOST_PATHS: &[(RoutePathClaim<'static>, &str)] = &[
    (
        RoutePathClaim {
            path: "/play",
            scope: RoutePathScope::Exact,
            decode: RoutePathDecode::Raw,
        },
        "/play",
    ),
    (
        RoutePathClaim {
            path: "/_uhura/editor",
            scope: RoutePathScope::Exact,
            decode: RoutePathDecode::Raw,
        },
        "/_uhura/editor",
    ),
    (
        RoutePathClaim {
            path: "/api",
            scope: RoutePathScope::Descendants,
            decode: RoutePathDecode::Raw,
        },
        "/api/*",
    ),
    (
        RoutePathClaim {
            path: "/assets",
            scope: RoutePathScope::Namespace,
            decode: RoutePathDecode::PercentDecodedOnce,
        },
        "/assets and /assets/*",
    ),
    (
        RoutePathClaim {
            path: "/favicon.ico",
            scope: RoutePathScope::Exact,
            decode: RoutePathDecode::PercentDecodedOnce,
        },
        "/favicon.ico",
    ),
];

fn standalone_web_host_route_collision(
    routes: &[CheckedRoutePattern],
) -> Option<(&CheckedRoutePattern, &'static str)> {
    routes.iter().find_map(|route| {
        STANDALONE_WEB_HOST_PATHS
            .iter()
            .find(|(claim, _)| route.overlaps(*claim))
            .map(|(_, label)| (route, *label))
    })
}

fn admit_play(
    snapshot: &ProjectSourceSnapshot,
    program: &Program,
    selector_packages: &BTreeMap<String, String>,
) -> Result<PlayAuthority, serde_json::Value> {
    let host_toml = snapshot
        .files
        .text("host.toml")
        .map_err(|message| host_failure("R3014", "uhura/host-manifest", message))?
        .ok_or_else(|| {
            host_failure(
                "R3014",
                "uhura/host-manifest",
                "host.toml is required for a current Uhura Play entry",
            )
        })?;
    let mut deployment = parse_host_manifest(&host_toml)
        .map_err(|message| host_failure("R3014", "uhura/host-manifest", message))?;
    resolve_deployment_selectors(program, selector_packages, &mut deployment)
        .map_err(|message| host_failure("R3014", "uhura/host-manifest", message))?;
    let admission = validate_deployment(program, &deployment)
        .map_err(|(code, message)| host_failure(code, "uhura/host-admission", message))?;
    let stylesheet = deployment
        .stylesheet
        .as_deref()
        .map(|path| {
            snapshot
                .files
                .text(path)
                .map_err(|message| host_failure("R3014", "uhura/stylesheet", message))?
                .ok_or_else(|| {
                    host_failure(
                        "R3014",
                        "uhura/stylesheet",
                        format!("configured stylesheet `{path}` is missing"),
                    )
                })
        })
        .transpose()?
        .unwrap_or_default();
    let provider_js = deployment
        .provider
        .as_ref()
        .map(|provider| {
            snapshot
                .files
                .text(&provider.module)
                .map_err(|message| host_failure("R3014", "uhura/provider", message))?
                .ok_or_else(|| {
                    host_failure(
                        "R3014",
                        "uhura/provider",
                        format!(
                            "configured provider module `{}` is missing",
                            provider.module
                        ),
                    )
                })
        })
        .transpose()?;
    let identity = deployment_identity(
        program,
        &deployment,
        &admission.configuration,
        &admission.ports,
        &stylesheet,
        provider_js.as_deref(),
    )
    .map_err(|message| host_failure("R3014", "uhura/deployment-identity", message))?;
    Ok(PlayAuthority {
        deployment,
        identity,
        configuration: admission.configuration,
        admitted_ports: admission.ports,
        stylesheet,
        provider_js,
    })
}

fn build_editor(
    revision: u64,
    checked: &CheckedProject,
    authority: Option<&PlayAuthority>,
    diagnostics: serde_json::Value,
) -> Result<EditorBuildArtifact, serde_json::Value> {
    let render = editor_render(
        revision,
        &checked.program,
        authority,
        &EditorInputs {
            evidence: &checked.evidence,
            sources: &checked.sources,
            provenance: &checked.provenance,
            semantic_provenance: &checked.semantic_provenance,
            interaction_graph: &checked.interaction_graph,
            graph_sources: &checked.graph_sources,
            authoring: &checked.authoring,
            assets: &checked.editor_assets,
        },
    )?;
    let preview_count = render.previews.len();
    let replay_derived_count = render
        .previews
        .iter()
        .filter(|preview| preview.derived)
        .count();
    Ok(EditorBuildArtifact {
        render,
        icon_fonts: Some(IconFontResources::from(&checked.icon_fonts)),
        preview_count,
        replay_derived_count,
        diagnostics,
    })
}

fn build_play(checked: &CheckedProject, authority: PlayAuthority) -> GoodBuild {
    let PlayAuthority {
        deployment,
        identity,
        configuration,
        admitted_ports,
        stylesheet,
        provider_js,
    } = authority;
    let config_json = play_config(
        &deployment,
        &identity,
        &configuration,
        admitted_ports,
        provider_js.as_deref(),
    );
    let evidence = EvidenceSummary::from_report(&checked.evidence).to_json();
    let inspect_json = to_canonical_json(&serde_json::json!({
        "protocol": UHURA_INSPECTION_PROTOCOL,
        "identityProtocol": identity.protocol,
        "entry": deployment.entry,
        "machine": deployment.machine,
        "presentation": deployment.presentation,
        "machineProgramHash": identity.machine_program_hash,
        "presentationHash": identity.presentation_hash,
        "evidenceHash": identity.evidence_hash,
        "deploymentHash": identity.deployment_hash,
        "sources": checked.provenance,
        "provenance": checked.semantic_provenance,
        "interactionGraph": checked.interaction_graph,
        "graphSources": checked.graph_sources,
        "evidence": evidence,
    }));

    GoodBuild {
        diagnostics: checked.diagnostics.clone(),
        ir: checked.program.to_canonical_string(),
        inspect_json,
        stylesheet,
        config_json,
        provider_js,
        play_assets: checked.play_assets.clone(),
        icon_fonts: Some(IconFontResources::from(&checked.icon_fonts)),
    }
}

fn checked_route_patterns(program: &Program, deployment: &Deployment) -> Vec<CheckedRoutePattern> {
    let Some(machine) = program.machine_program.machines.get(&deployment.machine) else {
        return Vec::new();
    };
    machine
        .ports
        .iter()
        .filter(|port| {
            deployment
                .ports
                .get(&port.name)
                .is_some_and(|adapter| adapter == WEB_HISTORY_ADAPTER)
        })
        .filter_map(|port| {
            let Some(Expr::Name { name }) = &port.configuration else {
                return None;
            };
            program.route_tables.get_key_value(name).or_else(|| {
                program
                    .route_tables
                    .iter()
                    .find(|(candidate, _)| candidate.ends_with(&format!("::{name}")))
            })
        })
        .flat_map(|(table, routes)| {
            routes.patterns().iter().map(move |route| {
                let checked_path = routes
                    .checked_paths()
                    .iter()
                    .find(|path| path.constructor() == route.constructor)
                    .expect("checked route table retains one path per pattern");
                CheckedRoutePattern {
                    table: table.clone(),
                    constructor: route.constructor.clone(),
                    pattern: route.pattern.clone(),
                    path: CheckedRoutePath::from_checked_parts(checked_path.parts()),
                }
            })
        })
        .collect()
}

fn host_failure(code: &str, rule: &str, message: impl Into<String>) -> serde_json::Value {
    let message = message.into();
    serde_json::json!({
        "format": "uhura-diagnostics",
        "version": 0,
        "summary": { "errors": 1, "warnings": 0 },
        "diagnostics": [{
            "code": code,
            "rule": rule,
            "severity": "error",
            "message": message,
        }],
    })
}

fn merge_diagnostics(envelopes: impl IntoIterator<Item = serde_json::Value>) -> serde_json::Value {
    let diagnostics = envelopes
        .into_iter()
        .flat_map(|envelope| {
            envelope
                .get("diagnostics")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        return serde_json::Value::Null;
    }
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["severity"] == "error")
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["severity"] == "warning")
        .count();
    serde_json::json!({
        "format": "uhura-diagnostics",
        "version": 0,
        "summary": {
            "errors": errors,
            "warnings": warnings,
        },
        "diagnostics": diagnostics,
    })
}

fn evidence_failure(
    report: &EvidenceReport,
    source_map: &SourceMap,
    sources: &[(String, String)],
) -> serde_json::Value {
    let diagnostics = report
        .failures
        .iter()
        .map(|failure| {
            Diagnostic::new(
                "R3013",
                "uhura/evidence",
                Severity::Error,
                failure.message.clone(),
                evidence_span(&failure.source, sources),
            )
        })
        .collect::<Vec<_>>();
    let mut envelope = to_envelope(&diagnostics, source_map);
    if let Some(entries) = envelope["diagnostics"].as_array_mut() {
        for (entry, failure) in entries.iter_mut().zip(&report.failures) {
            entry["sourceId"] = serde_json::json!(failure.source_id);
            entry["source"] = serde_json::to_value(&failure.source)
                .expect("Uhura evidence source is serializable");
            entry["scenario"] = serde_json::json!(failure.scenario);
            entry["stepIndex"] = serde_json::json!(failure.step_index);
        }
    }
    envelope
}

fn evidence_span(source: &uhura_core::ir::SourceRef, sources: &[(String, String)]) -> Span {
    sources
        .iter()
        .position(|(path, _)| path == &source.path)
        .filter(|index| {
            source.start <= source.end && source.end as usize <= sources[*index].1.len()
        })
        .map_or_else(
            || Span::new(FileId(0), 0, 0),
            |file| Span::new(FileId(file as u32), source.start, source.end),
        )
}

fn resolve_deployment_selectors(
    program: &Program,
    packages: &BTreeMap<String, String>,
    deployment: &mut Deployment,
) -> Result<(), String> {
    deployment.machine = resolve_host_selector(
        &deployment.machine,
        "machine",
        packages,
        program.machine_program.machines.keys().map(String::as_str),
    )?;
    if let Some(presentation) = deployment.presentation.as_mut() {
        *presentation = resolve_host_selector(
            presentation,
            "presentation",
            packages,
            program.presentations.keys().map(String::as_str),
        )?;
    }
    Ok(())
}

fn resolve_host_selector<'a>(
    selector: &str,
    kind: &str,
    packages: &BTreeMap<String, String>,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Result<String, String> {
    let parts = selector.split("::").collect::<Vec<_>>();
    if parts.len() != 2
        || parts[0].is_empty()
        || parts[1].is_empty()
        || !parts[1]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!(
            "Uhura 0.4 host {kind} selector `{selector}` must be `crate::PublicName` or `dependency_alias::PublicName`; logical-module-qualified selectors are not admitted"
        ));
    }
    let package = packages.get(parts[0]).ok_or_else(|| {
        format!(
            "Uhura 0.4 host {kind} selector `{selector}` uses unknown package alias `{}`",
            parts[0]
        )
    })?;
    let resolved = format!("{package}::{}", parts[1]);
    if candidates
        .into_iter()
        .any(|candidate| candidate == resolved)
    {
        Ok(resolved)
    } else {
        Err(format!(
            "Uhura 0.4 host {kind} selector `{selector}` resolves to unknown public declaration `{resolved}`"
        ))
    }
}

fn parse_host_manifest(source: &str) -> Result<Deployment, String> {
    let root = source
        .parse::<toml::Table>()
        .map_err(|error| format!("host.toml: {error}"))?;
    require_toml_keys(&root, "host.toml", &["entry"])?;
    let entries = root
        .get("entry")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "host.toml.entry must be a table".to_string())?;
    if entries.len() != 1 {
        return Err(format!(
            "host.toml.entry must declare exactly one entry, found {}",
            entries.len()
        ));
    }
    let (entry, value) = entries.iter().next().expect("one entry");
    if entry.is_empty()
        || entry.trim() != entry
        || entry
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(format!(
            "host.toml entry name `{entry}` is not a stable identity"
        ));
    }
    let table = value
        .as_table()
        .ok_or_else(|| format!("host.toml.entry.{entry} must be a table"))?;
    require_toml_keys(
        table,
        &format!("host.toml.entry.{entry}"),
        &[
            "machine",
            "presentation",
            "lifetime",
            "configuration",
            "ports",
            "stylesheet",
            "provider",
        ],
    )?;
    for required in ["machine", "lifetime"] {
        if !table.contains_key(required) {
            return Err(format!("host.toml.entry.{entry}.{required} is required"));
        }
    }
    let string = |name: &str| -> Result<String, String> {
        table
            .get(name)
            .and_then(toml::Value::as_str)
            .filter(|value| !value.is_empty() && value.trim() == *value)
            .map(str::to_owned)
            .ok_or_else(|| {
                format!(
                    "host.toml.entry.{entry}.{name} must be a non-empty text value without edge whitespace"
                )
            })
    };
    let lifetime = string("lifetime")?;
    if lifetime != "application-session" {
        return Err(format!(
            "host.toml.entry.{entry}.lifetime must be `application-session`, got `{lifetime}`"
        ));
    }
    let mut ports = BTreeMap::new();
    if let Some(ports_value) = table.get("ports") {
        let ports_table = ports_value
            .as_table()
            .ok_or_else(|| format!("host.toml.entry.{entry}.ports must be a table"))?;
        for (port, adapter) in ports_table {
            if !valid_port_locator(port) {
                return Err(format!(
                    "host.toml.entry.{entry}.ports contains invalid port name `{port}`"
                ));
            }
            let adapter = adapter.as_str().ok_or_else(|| {
                format!("host.toml.entry.{entry}.ports.{port} must be an adapter identity")
            })?;
            if adapter.is_empty() || adapter.trim() != adapter {
                return Err(format!(
                    "host.toml.entry.{entry}.ports.{port} must be a non-empty adapter identity without edge whitespace"
                ));
            }
            ports.insert(port.clone(), adapter.to_string());
        }
    }
    let provider = table
        .get("provider")
        .map(|value| {
            let provider_path = format!("host.toml.entry.{entry}.provider");
            let provider = value
                .as_table()
                .ok_or_else(|| format!("{provider_path} must be a table"))?;
            require_toml_keys(provider, &provider_path, &["module", "config"])?;
            let module = provider
                .get("module")
                .and_then(toml::Value::as_str)
                .filter(|value| !value.is_empty() && value.trim() == *value)
                .ok_or_else(|| {
                    format!("{provider_path}.module must be a non-empty project-relative path")
                })?
                .to_string();
            let config = provider
                .get("config")
                .map(|value| {
                    if !value.is_table() {
                        return Err(format!("{provider_path}.config must be a table"));
                    }
                    serde_json::to_value(value)
                        .map_err(|error| format!("{provider_path}.config: {error}"))
                })
                .transpose()?
                .unwrap_or_else(|| serde_json::json!({}));
            try_to_canonical_json(&config).map_err(|error| {
                format!("{provider_path}.config contains noncanonical deterministic data: {error}")
            })?;
            Ok::<ApplicationProvider, String>(ApplicationProvider { module, config })
        })
        .transpose()?;
    Ok(Deployment {
        entry: entry.clone(),
        machine: string("machine")?,
        presentation: table
            .get("presentation")
            .map(|_| string("presentation"))
            .transpose()?,
        lifetime,
        configuration: table
            .get("configuration")
            .map(|_| string("configuration"))
            .transpose()?,
        ports,
        stylesheet: table
            .get("stylesheet")
            .map(|_| string("stylesheet"))
            .transpose()?,
        provider,
    })
}

fn valid_port_locator(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            let mut bytes = segment.bytes();
            matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
                && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
        })
}

fn require_toml_keys(table: &toml::Table, path: &str, allowed: &[&str]) -> Result<(), String> {
    if let Some(key) = table.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!(
            "{path}.{key} is not allowed by the closed Uhura host schema"
        ));
    }
    Ok(())
}

fn host_adapter(identity: &str) -> Option<HostAdapter> {
    match identity {
        WEB_HISTORY_ADAPTER => Some(HostAdapter::WebHistory),
        APPLICATION_PROVIDER_ADAPTER => Some(HostAdapter::ApplicationProvider),
        _ => None,
    }
}

fn host_binding(
    adapter: HostAdapter,
    adapter_identity: &str,
    port: &uhura_core::ir::PortDef,
) -> Result<uhura_port::PortBinding, String> {
    let checked = port.contract_instance.as_ref().ok_or_else(|| {
        format!(
            "port `{}` did not retain its checked contract instance",
            port.name
        )
    })?;
    if adapter == HostAdapter::WebHistory
        && checked.identity.to_string() != "uhura.web_router@1::Router"
    {
        return Err(format!(
            "adapter `{adapter_identity}` requires uhura.web_router@1::Router, but port `{}` resolved {}",
            port.name, checked.identity
        ));
    }
    uhura_port::PortBinding::for_instance(&port.name, adapter_identity, checked)
        .map_err(|error| error.to_string())
}

fn validate_deployment(
    program: &Program,
    deployment: &Deployment,
) -> Result<DeploymentAdmission, (&'static str, String)> {
    program.validate_protocol().map_err(|message| {
        (
            "R3015",
            format!("invalid checked Uhura port contract: {message}"),
        )
    })?;
    let machine = program
        .machine_program
        .machines
        .get(&deployment.machine)
        .ok_or_else(|| {
            (
                "R3014",
                format!(
                    "host entry names unknown Uhura machine `{}`",
                    deployment.machine
                ),
            )
        })?;
    let configuration = match deployment.configuration.as_deref() {
        None if machine.config == uhura_core::TypeRef::Unit => Value::Unit,
        None => {
            return Err((
                "R3014",
                format!(
                    "host entry `{}` must provide `configuration` because Uhura machine `{}` requires {}",
                    deployment.entry,
                    deployment.machine,
                    machine.config.canonical_name()
                ),
            ));
        }
        Some(source) => {
            let json = serde_json::from_str::<serde_json::Value>(source).map_err(|error| {
                (
                    "R3014",
                    format!(
                        "host entry `{}` configuration is not tagged Uhura JSON: {error}",
                        deployment.entry
                    ),
                )
            })?;
            let canonical = try_to_canonical_json(&json).map_err(|error| {
                (
                    "R3014",
                    format!(
                        "host entry `{}` configuration is not canonical Uhura data: {error}",
                        deployment.entry
                    ),
                )
            })?;
            if canonical != source {
                return Err((
                    "R3014",
                    format!(
                        "host entry `{}` configuration must be canonical exact tagged Uhura JSON",
                        deployment.entry
                    ),
                ));
            }
            program
                .machine_program
                .decode_wire_value(&machine.config, &json)
                .map_err(|error| {
                    (
                        "R3014",
                        format!(
                            "host entry `{}` configuration does not satisfy {}: {error}",
                            deployment.entry,
                            machine.config.canonical_name()
                        ),
                    )
                })?
        }
    };
    program
        .machine_program
        .admit(
            &deployment.machine,
            configuration.clone(),
            format!("host-validation/entry/{}", deployment.entry),
        )
        .map_err(|error| {
            (
                "R3014",
                format!(
                    "host entry `{}` configuration cannot create Uhura machine genesis for `{}`: {error}",
                    deployment.entry, deployment.machine
                ),
            )
        })?;
    if let Some(presentation_id) = deployment.presentation.as_deref() {
        let presentation = program.presentations.get(presentation_id).ok_or_else(|| {
            (
                "R3014",
                format!("host entry names unknown Uhura presentation `{presentation_id}`"),
            )
        })?;
        if presentation.machine != deployment.machine {
            return Err((
                "R3014",
                format!(
                    "Uhura presentation `{presentation_id}` targets `{}`, not `{}`",
                    presentation.machine, deployment.machine
                ),
            ));
        }
    }
    let declared = machine
        .ports
        .iter()
        .map(|port| port.name.as_str())
        .collect::<BTreeSet<_>>();
    let bound = deployment
        .ports
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if declared != bound {
        let missing = declared.difference(&bound).copied().collect::<Vec<_>>();
        let extra = bound.difference(&declared).copied().collect::<Vec<_>>();
        return Err((
            "R3005",
            format!(
                "host port bindings must exactly match machine `{}`; missing [{}], extra [{}]",
                deployment.machine,
                missing.join(", "),
                extra.join(", ")
            ),
        ));
    }
    let application_provider_ports = deployment
        .ports
        .iter()
        .filter_map(|(port, adapter)| {
            (adapter == APPLICATION_PROVIDER_ADAPTER).then_some(port.as_str())
        })
        .collect::<Vec<_>>();
    match (
        application_provider_ports.is_empty(),
        deployment.provider.is_some(),
    ) {
        (true, true) => {
            return Err((
                "R3015",
                format!(
                    "host entry `{}` configures a provider module but binds no port to `{APPLICATION_PROVIDER_ADAPTER}`",
                    deployment.entry
                ),
            ));
        }
        (false, false) => {
            return Err((
                "R3015",
                format!(
                    "host entry `{}` binds provider port(s) [{}] but has no provider module",
                    deployment.entry,
                    application_provider_ports.join(", ")
                ),
            ));
        }
        _ => {}
    }

    let web_history_ports = deployment
        .ports
        .iter()
        .filter_map(|(port, adapter)| (adapter == WEB_HISTORY_ADAPTER).then_some(port.as_str()))
        .collect::<Vec<_>>();
    if web_history_ports.len() > 1 {
        return Err((
            "R3015",
            format!(
                "sealed host adapter capability `{WEB_HISTORY_ADAPTER}` may bind at most one machine port; found bindings [{}]",
                web_history_ports.join(", ")
            ),
        ));
    }

    let declarations = machine
        .ports
        .iter()
        .map(|port| {
            let contract = port.contract_instance.clone().ok_or_else(|| {
                (
                    "R3015",
                    format!(
                        "port `{}` did not retain its checked contract instance",
                        port.name
                    ),
                )
            })?;
            uhura_port::PortDeclaration::new(&port.name, contract)
                .map_err(|error| ("R3015", error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut bindings = Vec::with_capacity(machine.ports.len());
    for port in &machine.ports {
        let adapter_identity = &deployment.ports[&port.name];
        let adapter = host_adapter(adapter_identity).ok_or_else(|| {
            (
                "R3015",
                format!(
                    "host adapter `{adapter_identity}` is not in the sealed Uhura adapter table"
                ),
            )
        })?;
        bindings.push(
            host_binding(adapter, adapter_identity, port).map_err(|message| ("R3015", message))?,
        );
    }
    let admitted_set = uhura_port::admit_bindings(&declarations, &bindings)
        .into_result()
        .map_err(|issues| {
            let code = issues
                .first()
                .map_or("R3015", |issue| issue.code.diagnostic_code());
            let message = issues
                .into_iter()
                .map(|issue| issue.message)
                .collect::<Vec<_>>()
                .join("; ");
            (code, message)
        })?;
    let mut admitted = admitted_set
        .bindings
        .into_iter()
        .map(|binding| {
            let port = machine
                .ports
                .iter()
                .find(|port| port.name == binding.port)
                .expect("admission only returns declared ports");
            serde_json::json!({
                "port": binding.port,
                "adapter": binding.adapter,
                "contractHash": port.contract_hash,
                "contractInstanceHash": binding.contract_instance_hash,
            })
        })
        .collect::<Vec<_>>();
    admitted.sort_by(|left, right| left["port"].as_str().cmp(&right["port"].as_str()));
    Ok(DeploymentAdmission {
        configuration,
        ports: admitted,
    })
}

fn deployment_identity(
    program: &Program,
    deployment: &Deployment,
    configuration: &Value,
    admitted_ports: &[serde_json::Value],
    stylesheet: &str,
    provider_js: Option<&str>,
) -> Result<DeploymentIdentity, String> {
    if program.machine_program.language != "uhura 0.4"
        || program.machine_program.identity_protocol != MACHINE_PROGRAM_ID_PROTOCOL
    {
        return Err(format!(
            "unsupported Uhura machine identity protocol `{}` for language `{}`",
            program.machine_program.identity_protocol, program.machine_program.language
        ));
    }
    let machine_program_hash = program
        .machine_program
        .program_hashes
        .get(&deployment.machine)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Uhura machine `{}` has no frozen program identity",
                deployment.machine
            )
        })?;
    let presentation_hash = deployment
        .presentation
        .as_ref()
        .map(|presentation| {
            program
                .presentation_hashes
                .get(presentation)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Uhura presentation `{presentation}` has no frozen presentation identity"
                    )
                })
        })
        .transpose()?;
    let evidence_hash = program.evidence_hashes.get(&deployment.machine).cloned();
    let port_bindings = admitted_ports
        .iter()
        .map(|binding| {
            let field = |name: &str| {
                binding
                    .get(name)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        format!("admitted Uhura adapter is missing its `{name}` identity field")
                    })
            };
            Ok(DeploymentPortBinding {
                port: field("port")?,
                adapter: field("adapter")?,
                required_contract_hash: field("contractHash")?,
                admitted_contract_instance_hash: field("contractInstanceHash")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let provider = match (&deployment.provider, provider_js) {
        (Some(provider), Some(source)) => Some(DeploymentContentIdentity {
            protocol: UHURA_ADAPTER_PROVIDER_PROTOCOL.into(),
            configuration: provider.config.clone(),
            content_hash: sha256_hex(source.as_bytes()),
        }),
        (Some(provider), None) => {
            return Err(format!(
                "configured provider module `{}` has no selected content for DeploymentId",
                provider.module
            ));
        }
        (None, _) => None,
    };
    let material = DeploymentIdentityMaterial {
        machine: deployment.machine.clone(),
        machine_program_id: machine_program_hash.clone(),
        presentation: deployment
            .presentation
            .as_ref()
            .map(|id| DeploymentPresentationIdentity {
                id: id.clone(),
                presentation_id: presentation_hash
                    .clone()
                    .expect("selected presentation hash was resolved above"),
            }),
        entry: deployment.entry.clone(),
        lifetime: deployment.lifetime.clone(),
        configuration: configuration.to_wire_json(),
        port_bindings,
        stylesheet: deployment
            .stylesheet
            .as_ref()
            .map(|_| DeploymentContentIdentity {
                protocol: UHURA_STYLESHEET_CONTENT_PROTOCOL.into(),
                configuration: serde_json::Value::Null,
                content_hash: sha256_hex(stylesheet.as_bytes()),
            }),
        provider,
    };
    let deployment_hash = deployment_hash(&material)?;
    Ok(DeploymentIdentity {
        protocol: program.machine_program.identity_protocol.clone(),
        machine_program_hash,
        presentation_hash,
        evidence_hash,
        deployment_hash,
    })
}

fn play_config(
    deployment: &Deployment,
    identity: &DeploymentIdentity,
    configuration: &Value,
    ports: Vec<serde_json::Value>,
    provider_js: Option<&str>,
) -> String {
    let provider = deployment
        .provider
        .as_ref()
        .zip(provider_js)
        .map(|(provider, source)| {
            serde_json::json!({
                "protocol": UHURA_ADAPTER_PROVIDER_PROTOCOL,
                "module": format!(
                    "/api/play/provider.js?sha256={}",
                    sha256_hex(source.as_bytes())
                ),
                "config": provider.config,
            })
        });
    let mut config = serde_json::json!({
        "protocol": UHURA_PLAY_CONFIG_PROTOCOL,
        "identityProtocol": identity.protocol,
        "entry": deployment.entry,
        "machine": deployment.machine,
        "presentation": deployment.presentation,
        "machineProgramHash": identity.machine_program_hash,
        "presentationHash": identity.presentation_hash,
        "evidenceHash": identity.evidence_hash,
        "deploymentHash": identity.deployment_hash,
        "lifetime": deployment.lifetime,
        "instance": format!("entry/{}", deployment.entry),
        "configuration": configuration.to_wire_json(),
        "ports": ports,
    });
    if let Some(provider) = provider {
        config["provider"] = provider;
    }
    to_canonical_json(&config)
}

fn source_provenance(sources: &[(String, String)]) -> Vec<serde_json::Value> {
    sources
        .iter()
        .enumerate()
        .map(|(file, (path, text))| {
            serde_json::json!({
                "file": file,
                "path": path,
                "sha256": sha256_hex(text.as_bytes()),
                "bytes": text.len(),
            })
        })
        .collect()
}

fn editor_render(
    revision: u64,
    program: &Program,
    authority: Option<&PlayAuthority>,
    inputs: &EditorInputs<'_>,
) -> Result<EditorRender, serde_json::Value> {
    let EditorInputs {
        evidence,
        sources,
        provenance,
        semantic_provenance,
        interaction_graph,
        graph_sources,
        authoring,
        ..
    } = inputs;
    let mut source_map = SourceMap::new();
    let source_files = sources
        .iter()
        .map(|(path, text)| (path.clone(), source_map.add(path.clone(), text.clone())))
        .collect::<BTreeMap<_, _>>();
    semantic_provenance.validate().map_err(|error| {
        host_failure(
            "R3014",
            "uhura/editor-authoring",
            format!("semantic provenance is invalid: {error}"),
        )
    })?;
    let mut previews = Vec::new();
    let presentation_kinds = presentation_kinds(evidence);
    let presentation_sources =
        presentation_authoring_sources(program, &presentation_kinds, semantic_provenance)?;
    let (mut authoring_targets, authoring_entries) =
        checked_authoring_metadata(authoring, &source_map, &source_files, &presentation_sources)?;
    let presentation_node_owners = presentation_node_owners(program);
    let projection_authoring = ProjectionAuthoringContext {
        source_map: &source_map,
        source_files: &source_files,
        presentation_sources: &presentation_sources,
        presentation_node_owners: &presentation_node_owners,
        provenance: semantic_provenance,
        checked_targets: &authoring.targets,
    };
    let mut default_presentations = BTreeSet::new();
    let declared_defaults = evidence
        .artifacts
        .examples
        .values()
        .filter_map(|example| {
            example
                .metadata
                .is_default
                .then(|| example.metadata.presentation.clone())
                .flatten()
        })
        .collect::<BTreeSet<_>>();
    for (name, example) in &evidence.artifacts.examples {
        let presentations = match example.metadata.presentation.as_deref() {
            Some(presentation_id) => program
                .presentations
                .get(presentation_id)
                .into_iter()
                .collect::<Vec<_>>(),
            None => program
                .presentations
                .values()
                .filter(|presentation| presentation.machine == example.snapshot.machine)
                .collect::<Vec<_>>(),
        };
        let instance = (!presentations.is_empty())
            .then(|| {
                instance_from_snapshot(program, &example.snapshot).map_err(|message| {
                    host_failure(
                        "R3010",
                        "uhura/editor-restore",
                        format!("example `{name}` snapshot is not restorable: {message}"),
                    )
                })
            })
            .transpose()?;
        for presentation in presentations {
            let instance = instance
                .as_ref()
                .expect("a projected evidence example has a restored instance");
            let projection = program
                .project(instance, &presentation.id)
                .map_err(|error| {
                    host_failure(
                        "R3006",
                        "uhura/editor-projection",
                        format!(
                            "example `{name}` could not project through `{}`: {error}",
                            presentation.id
                        ),
                    )
                })?;
            let interactions =
                projection_interactions(&projection.document.nodes, &example.snapshot.machine);
            let pin_key = format!("{}::{}", example.reference.scenario, example.reference.pin);
            let pin = evidence
                .artifacts
                .pins
                .get(&pin_key)
                .expect("a resolved evidence example retains its published pin");
            let source_id = pin.source_id.clone();
            let mut snapshot = serde_json::to_value(&example.snapshot)
                .expect("Uhura evidence snapshots are serializable");
            exactify_sequences(&mut snapshot);
            let scenario_receipts =
                pin_receipt_log(evidence, &example.reference.scenario, &example.snapshot);
            let kind = preview_kind(example.metadata.kind);
            let identity = PreviewIdentity {
                kind,
                subject: presentation.id.clone(),
                example: name.clone(),
            };
            let preview_id = stable_preview_id(&identity);
            let preview_provenance = projection_provenance(
                &preview_id,
                &presentation.id,
                &projection.sources,
                &projection_authoring,
                &mut authoring_targets,
            )?;
            let is_default = example.metadata.is_default
                || (!declared_defaults.contains(&presentation.id)
                    && default_presentations.insert(presentation.id.clone()));
            previews.push(Preview {
                id: preview_id,
                identity,
                source_file: presentation_sources
                    .get(&presentation.id)
                    .expect("every checked presentation has an authoring source")
                    .path
                    .clone(),
                is_default,
                pinned: true,
                derived: false,
                in_flight: example.snapshot.inbox.len(),
                from: None,
                replay_steps: Vec::new(),
                replay: Vec::new(),
                note: example.metadata.note.clone(),
                data: preview_data(&example.snapshot),
                interactions,
                documentation: PreviewDocumentation::default(),
                provenance: preview_provenance,
                evidence: Some(PreviewEvidence {
                    scenario: example.reference.scenario.clone(),
                    pin: example.reference.pin.clone(),
                    source_id,
                    registration_source: serde_json::to_value(&example.source)
                        .expect("Uhura evidence sources are serializable"),
                    pin_source: serde_json::to_value(&pin.source)
                        .expect("Uhura evidence sources are serializable"),
                    observation: example.observation.to_wire_json(),
                    snapshot,
                    scenario_receipt_log: scenario_receipts,
                }),
                content: PreviewContent::Projection(projection.into()),
            });
        }
    }
    establish_preview_lineage(program, evidence, semantic_provenance, &mut previews);

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

    let mut checkpoints = serde_json::to_value(&evidence.artifacts.checkpoints)
        .expect("Uhura evidence checkpoints are serializable");
    exactify_sequences(&mut checkpoints);
    let evidence_summary = EvidenceSummary::from_report(evidence).to_json();

    let deployment = authority.map(|authority| MachineDeployment {
        entry: authority.deployment.entry.clone(),
        machine: authority.deployment.machine.clone(),
        presentation: authority.deployment.presentation.clone(),
        instance: format!("entry/{}", authority.deployment.entry),
        machine_program_hash: authority.identity.machine_program_hash.clone(),
        presentation_hash: authority.identity.presentation_hash.clone(),
        evidence_hash: authority.identity.evidence_hash.clone(),
        deployment_hash: authority.identity.deployment_hash.clone(),
    });
    let machine = MachineSidecar::new(MachineSidecarInput {
        identity_protocol: program.machine_program.identity_protocol.clone(),
        deployment,
        sources: serde_json::Value::Array(provenance.to_vec()),
        provenance: (*semantic_provenance).clone(),
        interaction_graph: (*interaction_graph).clone(),
        graph_sources: (*graph_sources).clone(),
        checkpoints,
        evidence: evidence_summary,
    });
    let application_name = authority
        .map(|authority| authority.deployment.entry.clone())
        .or_else(|| program.machine_program.modules.first().cloned())
        .unwrap_or_else(|| "Uhura".to_string());
    let application_interaction_graph = application_interaction_graph(
        program,
        evidence,
        authority,
        application_name.clone(),
        &previews,
    );

    Ok(EditorRender {
        revision,
        freshness: RenderFreshness::Current,
        application: Application {
            name: application_name.clone(),
        },
        authoring: AuthoringMetadata {
            targets: sorted_authoring_targets(authoring_targets),
            entries: authoring_entries,
        },
        groups,
        previews,
        stylesheet: authority
            .map(|authority| authority.stylesheet.clone())
            .unwrap_or_default(),
        assets: inputs.assets.clone(),
        interaction_graph: application_interaction_graph,
        machine: Some(machine),
    })
}

fn preview_kind(kind: Option<uhura_core::EvidencePresentationKind>) -> PreviewKind {
    match kind {
        Some(uhura_core::EvidencePresentationKind::Component) => PreviewKind::Component,
        Some(uhura_core::EvidencePresentationKind::Surface) => PreviewKind::Surface,
        Some(uhura_core::EvidencePresentationKind::Page) | None => PreviewKind::Page,
    }
}

fn source_target_owner_kind(kind: PreviewKind) -> SourceTargetOwnerKind {
    match kind {
        PreviewKind::Component => SourceTargetOwnerKind::Component,
        PreviewKind::Page => SourceTargetOwnerKind::Page,
        PreviewKind::Surface => SourceTargetOwnerKind::Surface,
    }
}

fn presentation_kinds(evidence: &EvidenceReport) -> BTreeMap<String, PreviewKind> {
    let mut kinds = BTreeMap::new();
    for example in evidence.artifacts.examples.values() {
        let Some(presentation) = example.metadata.presentation.as_ref() else {
            continue;
        };
        let kind = preview_kind(example.metadata.kind);
        // Presentation taxonomy is example metadata in the retained evidence
        // language, not a declaration invariant. The checker therefore
        // admits the same UI declaration in more than one preview taxonomy.
        // SourceTarget has one declaration owner slot, so choose the stable
        // broadest taxonomy while preserving each preview's own kind.
        retain_presentation_owner_kind(&mut kinds, presentation, kind);
    }
    kinds
}

fn retain_presentation_owner_kind(
    kinds: &mut BTreeMap<String, PreviewKind>,
    presentation: &str,
    kind: PreviewKind,
) {
    kinds
        .entry(presentation.to_string())
        .and_modify(|current| *current = (*current).min(kind))
        .or_insert(kind);
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PresentationAuthoringSource {
    source_id: String,
    path: String,
    owner: SourceTargetOwner,
}

struct ProjectionAuthoringContext<'a> {
    source_map: &'a SourceMap,
    source_files: &'a BTreeMap<String, FileId>,
    presentation_sources: &'a BTreeMap<String, PresentationAuthoringSource>,
    presentation_node_owners: &'a BTreeMap<String, String>,
    provenance: &'a Provenance,
    checked_targets: &'a [AuthoringTarget],
}

fn checked_authoring_metadata(
    projection: &AuthoringProjection,
    source_map: &SourceMap,
    source_files: &BTreeMap<String, FileId>,
    presentation_sources: &BTreeMap<String, PresentationAuthoringSource>,
) -> Result<(BTreeMap<String, SourceTarget>, Vec<SourceMetadataEntry>), serde_json::Value> {
    projection.validate().map_err(|error| {
        host_failure(
            "R3014",
            "uhura/editor-authoring",
            format!("checked authoring projection is invalid: {error}"),
        )
    })?;
    let mut targets = BTreeMap::<String, SourceTarget>::new();
    for target in &projection.targets {
        let file = source_files.get(&target.file).copied().ok_or_else(|| {
            host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "checked authoring target `{}` refers to uncaptured file `{}`",
                    target.id, target.file
                ),
            )
        })?;
        let span = checked_editor_span(
            source_map,
            file,
            target.span.start,
            target.span.end,
            &target.id,
        )?;
        let presentation = presentation_sources.get(&target.owner).ok_or_else(|| {
            host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "checked authoring target `{}` has unknown presentation owner `{}`",
                    target.id, target.owner
                ),
            )
        })?;
        let class = match target.class {
            CheckedAuthoringTargetClass::UiElement => SourceTargetClass::UiElement,
            CheckedAuthoringTargetClass::IfBlock => SourceTargetClass::IfBlock,
            CheckedAuthoringTargetClass::EachBlock => SourceTargetClass::EachBlock,
        };
        let text = source_map.text(file);
        let start = usize::try_from(target.span.start).expect("validated authoring span start");
        let end = usize::try_from(target.span.end).expect("validated authoring span end");
        let editor_target = SourceTarget {
            id: target.id.clone(),
            class,
            file: target.file.clone(),
            span,
            label: source_target_label(&text[start..end], &target.label),
            owner: presentation.owner.clone(),
        };
        if targets
            .insert(editor_target.id.clone(), editor_target)
            .is_some()
        {
            return Err(host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!("checked authoring repeats target `{}`", target.id),
            ));
        }
    }

    let mut entries = Vec::with_capacity(projection.entries.len());
    for entry in &projection.entries {
        let target = targets.get(&entry.target_id).ok_or_else(|| {
            host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "checked authoring entry `{}` refers to unknown target `{}`",
                    entry.id, entry.target_id
                ),
            )
        })?;
        let file = source_files.get(&target.file).copied().ok_or_else(|| {
            host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "checked authoring entry `{}` refers to uncaptured file `{}`",
                    entry.id, target.file
                ),
            )
        })?;
        let class = match entry.class {
            CheckedAuthoringEntryClass::Annotation => SourceMetadataClass::Annotation,
        };
        entries.push(SourceMetadataEntry {
            id: entry.id.clone(),
            class,
            kind: entry.kind.clone(),
            text: entry.text.clone(),
            span: checked_editor_span(
                source_map,
                file,
                entry.span.start,
                entry.span.end,
                &entry.id,
            )?,
            target_id: entry.target_id.clone(),
            order: usize::try_from(entry.order).map_err(|_| {
                host_failure(
                    "R3014",
                    "uhura/editor-authoring",
                    format!(
                        "checked authoring entry `{}` order does not fit usize",
                        entry.id
                    ),
                )
            })?,
        });
    }
    entries.sort_by(|left, right| {
        let left_target = &targets[&left.target_id];
        let right_target = &targets[&right.target_id];
        left_target
            .file
            .cmp(&right_target.file)
            .then_with(|| left.span.offset.cmp(&right.span.offset))
            .then_with(|| left.order.cmp(&right.order))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok((targets, entries))
}

fn checked_editor_span(
    source_map: &SourceMap,
    file: FileId,
    start: u32,
    end: u32,
    identity: &str,
) -> Result<EditorSourceSpan, serde_json::Value> {
    let text = source_map.text(file);
    let start_index = usize::try_from(start).unwrap_or(usize::MAX);
    let end_index = usize::try_from(end).unwrap_or(usize::MAX);
    if start > end
        || end_index > text.len()
        || !text.is_char_boundary(start_index)
        || !text.is_char_boundary(end_index)
    {
        return Err(host_failure(
            "R3014",
            "uhura/editor-authoring",
            format!("checked authoring identity `{identity}` has invalid byte span {start}..{end}"),
        ));
    }
    Ok(EditorSourceSpan::from_span(
        source_map,
        Span::new(file, start, end),
    ))
}

fn presentation_authoring_sources(
    program: &Program,
    kinds: &BTreeMap<String, PreviewKind>,
    provenance: &Provenance,
) -> Result<BTreeMap<String, PresentationAuthoringSource>, serde_json::Value> {
    program
        .presentations
        .values()
        .map(|presentation| {
            let path = if presentation.source.path == "<resolved-project>" {
                presentation_source_path(provenance, &presentation.id).ok_or_else(|| {
                    host_failure(
                        "R3014",
                        "uhura/editor-authoring",
                        format!(
                            "presentation `{}` has no physical declaration provenance",
                            presentation.id
                        ),
                    )
                })?
            } else {
                presentation.source.path.clone()
            };
            let kind = kinds
                .get(&presentation.id)
                .copied()
                .unwrap_or(PreviewKind::Page);
            Ok((
                presentation.id.clone(),
                PresentationAuthoringSource {
                    source_id: presentation.source.id.clone(),
                    path,
                    owner: SourceTargetOwner {
                        kind: source_target_owner_kind(kind),
                        name: presentation.id.clone(),
                    },
                },
            ))
        })
        .collect()
}

fn source_is_within_presentation(source: &str, declaration: &str) -> bool {
    source == declaration
        || source
            .strip_prefix(declaration)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn presentation_node_owners(program: &Program) -> BTreeMap<String, String> {
    fn collect(
        nodes: &[uhura_core::UiNode],
        presentation: &str,
        output: &mut BTreeMap<String, String>,
    ) {
        for node in nodes {
            let (source, children) = match node {
                uhura_core::UiNode::Text { source, .. }
                | uhura_core::UiNode::Interpolation { source, .. } => (source, None),
                uhura_core::UiNode::Element {
                    source, children, ..
                }
                | uhura_core::UiNode::If {
                    source, children, ..
                }
                | uhura_core::UiNode::Each {
                    source, children, ..
                } => (source, Some(children.as_slice())),
                uhura_core::UiNode::Match { source, cases, .. } => {
                    if !source.id.is_empty() {
                        output.insert(source.id.clone(), presentation.to_owned());
                    }
                    for case in cases {
                        if !case.source.id.is_empty() {
                            output.insert(case.source.id.clone(), presentation.to_owned());
                        }
                        collect(&case.children, presentation, output);
                    }
                    continue;
                }
            };
            if !source.id.is_empty() {
                output.insert(source.id.clone(), presentation.to_owned());
            }
            if let Some(children) = children {
                collect(children, presentation, output);
            }
        }
    }

    let mut output = BTreeMap::new();
    for presentation in program.presentations.values() {
        collect(&presentation.nodes, &presentation.id, &mut output);
    }
    output
}

fn projection_authoring_source(
    source: &SourceRef,
    presentation: &str,
    context: &ProjectionAuthoringContext<'_>,
) -> Result<(String, SourceTargetOwner, u32, u32), serde_json::Value> {
    let matched_presentation = context
        .presentation_node_owners
        .get(&source.id)
        .map(String::as_str)
        .or_else(|| {
            context
                .presentation_sources
                .iter()
                .filter(|(_, candidate)| {
                    source_is_within_presentation(&source.id, &candidate.source_id)
                })
                .max_by_key(|(_, candidate)| candidate.source_id.len())
                .map(|(presentation, _)| presentation.as_str())
        })
        .unwrap_or(presentation);
    let candidate = context
        .presentation_sources
        .get(matched_presentation)
        .ok_or_else(|| {
            host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "semantic source `{}` resolves to unknown presentation `{matched_presentation}`",
                    source.id
                ),
            )
        })?;
    if source.path != "<resolved-project>" {
        if source.path != candidate.path {
            return Err(host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "semantic source `{}` disagrees with presentation source `{}`",
                    source.id, candidate.path
                ),
            ));
        }
        return Ok((
            candidate.path.clone(),
            candidate.owner.clone(),
            source.start,
            source.end,
        ));
    }

    let mut occurrences = context
        .provenance
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.node == source.id && occurrence.role == "definition")
        .filter_map(|occurrence| {
            context
                .provenance
                .sources
                .iter()
                .find(|candidate| candidate.source == occurrence.source)
                .map(|captured| (captured.path.as_str(), occurrence.start, occurrence.end))
        })
        .collect::<Vec<_>>();
    occurrences.sort();
    occurrences.dedup();
    let (path, start, end) = match occurrences.as_slice() {
        [occurrence] => *occurrence,
        [] => {
            return Err(host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "semantic source `{}` cannot be joined to authored UI provenance",
                    source.id
                ),
            ));
        }
        _ => {
            return Err(host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "semantic source `{}` resolves to more than one authored UI occurrence",
                    source.id
                ),
            ));
        }
    };
    if path != candidate.path {
        return Err(host_failure(
            "R3014",
            "uhura/editor-authoring",
            format!(
                "semantic source `{}` resolves to `{path}`, outside presentation source `{}`",
                source.id, candidate.path
            ),
        ));
    }
    Ok((path.to_owned(), candidate.owner.clone(), start, end))
}

fn projection_provenance(
    preview_id: &str,
    presentation: &str,
    projection: &uhura_core::ProjectionSources,
    context: &ProjectionAuthoringContext<'_>,
    authoring_targets: &mut BTreeMap<String, SourceTarget>,
) -> Result<PreviewProvenance, serde_json::Value> {
    let mut projected_sources = BTreeMap::<String, (SourceRef, Vec<String>)>::new();
    for (key, source) in &projection.nodes {
        if source.id.is_empty() {
            return Err(host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!("projection `{presentation}` node `{key}` has no semantic source identity"),
            ));
        }
        match projected_sources.entry(source.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((source.clone(), vec![key.clone()]));
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let (canonical, keys) = entry.get_mut();
                if canonical.path != source.path
                    || canonical.start != source.start
                    || canonical.end != source.end
                {
                    return Err(host_failure(
                        "R3014",
                        "uhura/editor-authoring",
                        format!(
                            "semantic source `{}` resolves to inconsistent physical spans",
                            source.id
                        ),
                    ));
                }
                keys.push(key.clone());
            }
        }
    }

    let mut occurrences = Vec::with_capacity(projected_sources.len());
    for (semantic_target_id, (source, mut keys)) in projected_sources {
        keys.sort();
        let (physical_path, owner, source_start, source_end) =
            projection_authoring_source(&source, presentation, context)?;
        let target_id = context
            .checked_targets
            .iter()
            .find(|target| {
                target.class == CheckedAuthoringTargetClass::UiElement
                    && target.owner == owner.name
                    && target.file == physical_path
                    && target.span.start == source_start
                    && target.span.end == source_end
            })
            .map(|target| target.id.clone())
            .unwrap_or(semantic_target_id);
        let file = context
            .source_files
            .get(physical_path.as_str())
            .copied()
            .ok_or_else(|| {
                host_failure(
                    "R3014",
                    "uhura/editor-authoring",
                    format!(
                        "semantic source `{}` refers to uncaptured file `{}`",
                        source.id, physical_path
                    ),
                )
            })?;
        let text = context.source_map.text(file);
        let start = source_start as usize;
        let end = source_end as usize;
        if start > end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return Err(host_failure(
                "R3014",
                "uhura/editor-authoring",
                format!(
                    "semantic source `{}` has invalid byte span {}..{} in `{}`",
                    source.id, source_start, source_end, physical_path
                ),
            ));
        }
        let span = EditorSourceSpan::from_span(
            context.source_map,
            Span::new(file, source_start, source_end),
        );
        let target = SourceTarget {
            id: target_id.clone(),
            class: SourceTargetClass::UiElement,
            file: physical_path,
            span,
            label: source_target_label(&text[start..end], &source.id),
            owner,
        };
        if let Some(existing) = authoring_targets.get(&target_id) {
            if existing.file != target.file || existing.span != target.span {
                return Err(host_failure(
                    "R3014",
                    "uhura/editor-authoring",
                    format!(
                        "semantic source `{target_id}` resolves to inconsistent authoring targets"
                    ),
                ));
            }
        } else {
            authoring_targets.insert(target_id.clone(), target);
        }
        occurrences.push(TargetOccurrence {
            id: sha256_hex(
                to_canonical_json(&serde_json::json!({
                    "preview": preview_id,
                    "target": target_id,
                    "keys": keys,
                }))
                .as_bytes(),
            ),
            target_id,
            anchors: keys,
        });
    }
    append_structural_authoring_occurrences(
        preview_id,
        presentation,
        projection,
        context,
        &mut occurrences,
    )?;
    Ok(PreviewProvenance { occurrences })
}

fn append_structural_authoring_occurrences(
    preview_id: &str,
    presentation: &str,
    projection: &uhura_core::ProjectionSources,
    context: &ProjectionAuthoringContext<'_>,
    occurrences: &mut Vec<TargetOccurrence>,
) -> Result<(), serde_json::Value> {
    for target in context.checked_targets.iter().filter(|target| {
        target.owner == presentation
            && matches!(
                target.class,
                CheckedAuthoringTargetClass::IfBlock | CheckedAuthoringTargetClass::EachBlock
            )
    }) {
        if occurrences
            .iter()
            .any(|occurrence| occurrence.target_id == target.id)
        {
            continue;
        }
        let mut candidates = Vec::<(u32, u32, String)>::new();
        for (key, source) in &projection.nodes {
            let (path, owner, start, end) =
                projection_authoring_source(source, presentation, context)?;
            if owner.name == target.owner
                && path == target.file
                && target.span.start <= start
                && end <= target.span.end
            {
                candidates.push((start, end, key.clone()));
            }
        }
        let mut anchors = candidates
            .iter()
            .filter(|(start, end, _)| {
                !candidates.iter().any(|(outer_start, outer_end, _)| {
                    outer_start <= start
                        && end <= outer_end
                        && (outer_start < start || end < outer_end)
                })
            })
            .map(|(_, _, key)| key.clone())
            .collect::<Vec<_>>();
        anchors.sort();
        anchors.dedup();
        if anchors.is_empty() {
            continue;
        }
        occurrences.push(TargetOccurrence {
            id: sha256_hex(
                to_canonical_json(&serde_json::json!({
                    "preview": preview_id,
                    "target": target.id,
                    "keys": anchors,
                }))
                .as_bytes(),
            ),
            target_id: target.id.clone(),
            anchors,
        });
    }
    occurrences.sort_by(|left, right| left.target_id.cmp(&right.target_id));
    Ok(())
}

fn presentation_source_path(provenance: &Provenance, presentation: &str) -> Option<String> {
    let name = presentation.rsplit("::").next()?;
    let declaration = semantic_node_id(presentation, "root", "ui", &format!("declaration/{name}"));
    let source = provenance
        .occurrences
        .iter()
        .find(|occurrence| {
            occurrence.node == declaration
                && occurrence.role == "definition"
                && occurrence.owner == "root"
        })?
        .source;
    provenance
        .sources
        .iter()
        .find(|candidate| candidate.source == source)
        .map(|candidate| candidate.path.clone())
}

fn sorted_authoring_targets(targets: BTreeMap<String, SourceTarget>) -> Vec<SourceTarget> {
    let mut targets = targets.into_values().collect::<Vec<_>>();
    targets.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| left.span.offset.cmp(&right.span.offset))
            .then_with(|| left.id.cmp(&right.id))
    });
    targets
}

fn source_target_label(source: &str, fallback: &str) -> String {
    let line = source
        .trim()
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(fallback);
    let mut chars = line.chars();
    let label = chars.by_ref().take(72).collect::<String>();
    if chars.next().is_some() {
        format!("{label}…")
    } else {
        label
    }
}

fn application_interaction_graph(
    program: &Program,
    evidence: &EvidenceReport,
    authority: Option<&PlayAuthority>,
    application_name: String,
    previews: &[Preview],
) -> InteractionGraph {
    let definitions = previews
        .iter()
        .map(|preview| (preview.identity.kind, preview.identity.subject.clone()))
        .collect::<BTreeSet<_>>();
    let mut nodes = definitions
        .iter()
        .filter_map(|(kind, presentation)| {
            let (prefix, kind) = match kind {
                PreviewKind::Page => ("page", NodeKind::Page),
                PreviewKind::Surface => ("surface", NodeKind::Surface),
                // The frozen board graph deliberately has no component node:
                // components remain independently inspectable preview groups,
                // but are not application destinations.
                PreviewKind::Component => return None,
            };
            Some(InteractionNode {
                id: format!("{prefix}:{presentation}"),
                kind,
                label: presentation.clone(),
                modality: None,
            })
        })
        .collect::<Vec<_>>();

    let navigation =
        application_navigation_graph(program, evidence, authority, &definitions, previews);
    nodes.extend(navigation.nodes);
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    nodes.dedup_by(|left, right| left.id == right.id);

    let mut edges = navigation.edges;
    edges.extend(application_surface_edges(previews));
    edges.sort_by(|left, right| {
        (
            &left.from,
            &left.to,
            &left.event,
            application_edge_rank(left.kind),
        )
            .cmp(&(
                &right.from,
                &right.to,
                &right.event,
                application_edge_rank(right.kind),
            ))
    });
    edges.dedup_by(|left, right| {
        left.kind == right.kind
            && left.from == right.from
            && left.to == right.to
            && left.event == right.event
    });
    for (index, edge) in edges.iter_mut().enumerate() {
        edge.id = format!("application/edge/{index}");
    }

    let entry = authority
        .and_then(|authority| authority.deployment.presentation.as_ref())
        .filter(|presentation| definitions.contains(&(PreviewKind::Page, (*presentation).clone())))
        .or_else(|| {
            definitions.iter().find_map(|(kind, presentation)| {
                (*kind == PreviewKind::Page).then_some(presentation)
            })
        })
        .map_or_else(String::new, |presentation| format!("page:{presentation}"));
    InteractionGraph {
        protocol: INTERACTION_GRAPH_PROTOCOL.to_string(),
        app: application_name,
        entry,
        nodes,
        edges,
    }
}

#[derive(Default)]
struct ApplicationNavigationGraph {
    nodes: Vec<InteractionNode>,
    edges: Vec<InteractionEdge>,
}

fn application_edge_rank(kind: ApplicationEdgeKind) -> u8 {
    match kind {
        ApplicationEdgeKind::Navigate => 0,
        ApplicationEdgeKind::NavigateBack => 1,
        ApplicationEdgeKind::Present => 2,
        ApplicationEdgeKind::Dismiss => 3,
        ApplicationEdgeKind::StateChange => 4,
        ApplicationEdgeKind::SendCommand => 5,
        ApplicationEdgeKind::ReceiveOutcome => 6,
    }
}

fn application_edge(
    kind: ApplicationEdgeKind,
    from: String,
    to: String,
    event: String,
) -> InteractionEdge {
    InteractionEdge {
        id: String::new(),
        kind,
        from,
        to,
        event,
        guard: None,
        command: None,
        outcome: None,
        source: None,
    }
}

fn application_navigation_graph(
    program: &Program,
    evidence: &EvidenceReport,
    authority: Option<&PlayAuthority>,
    definitions: &BTreeSet<(PreviewKind, String)>,
    previews: &[Preview],
) -> ApplicationNavigationGraph {
    let mut graph = ApplicationNavigationGraph::default();
    for machine in program.machine_program.machines.values() {
        for (port, route_type) in navigation_ports(machine, authority) {
            let destinations =
                route_presentation_destinations(evidence, machine, &route_type, definitions);
            if destinations.is_empty() {
                let fallback = application_preview_route_graph(
                    program,
                    machine,
                    &port,
                    &route_type,
                    definitions,
                    previews,
                );
                graph.nodes.extend(fallback.nodes);
                graph.edges.extend(fallback.edges);
                continue;
            }
            for presentation in program.presentations.values().filter(|presentation| {
                presentation.machine == machine.id
                    && definitions.contains(&(PreviewKind::Page, presentation.id.clone()))
            }) {
                let mut inputs = BTreeSet::new();
                collect_presentation_inputs(
                    &presentation.nodes,
                    machine.local_input.id(),
                    &mut inputs,
                );
                for input in inputs {
                    let Some(handler) = machine.handlers.get(&input) else {
                        continue;
                    };
                    let mut route_constructors = BTreeSet::new();
                    collect_handler_route_constructors(
                        &handler.body,
                        program,
                        machine,
                        &port,
                        &route_type,
                        &mut BTreeMap::new(),
                        &mut BTreeSet::new(),
                        &mut route_constructors,
                    );
                    for route in route_constructors {
                        for destination in destinations.get(&route).into_iter().flatten() {
                            graph.edges.push(application_edge(
                                ApplicationEdgeKind::Navigate,
                                format!("page:{}", presentation.id),
                                format!("page:{destination}"),
                                input.clone(),
                            ));
                        }
                    }
                }
            }
        }
    }
    graph
}

fn navigation_ports(machine: &Machine, authority: Option<&PlayAuthority>) -> Vec<(String, String)> {
    machine
        .ports
        .iter()
        .filter(|port| {
            port.contract == "uhura.web_router@1::Router"
                || authority.is_some_and(|authority| {
                    authority
                        .deployment
                        .ports
                        .get(&port.name)
                        .map(String::as_str)
                        == Some(WEB_HISTORY_ADAPTER)
                })
        })
        .filter_map(|port| {
            port.type_arguments
                .first()
                .map(|route| (port.name.clone(), route.canonical_name()))
        })
        .collect()
}

fn route_presentation_destinations(
    evidence: &EvidenceReport,
    machine: &Machine,
    route_type: &str,
    definitions: &BTreeSet<(PreviewKind, String)>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut destinations = BTreeMap::<String, BTreeSet<String>>::new();
    for example in evidence.artifacts.examples.values() {
        let Some(presentation) = example.metadata.presentation.as_ref() else {
            continue;
        };
        if !definitions.contains(&(PreviewKind::Page, presentation.clone()))
            || example.snapshot.machine != machine.id
        {
            continue;
        }
        let mut constructors = BTreeSet::new();
        collect_value_constructors(&example.snapshot.observation, route_type, &mut constructors);
        for constructor in constructors {
            destinations
                .entry(constructor)
                .or_default()
                .insert(presentation.clone());
        }
    }
    destinations
}

fn application_preview_route_graph(
    program: &Program,
    machine: &Machine,
    port: &str,
    route_type: &str,
    definitions: &BTreeSet<(PreviewKind, String)>,
    previews: &[Preview],
) -> ApplicationNavigationGraph {
    let page_presentations = program
        .presentations
        .values()
        .filter(|presentation| {
            presentation.machine == machine.id
                && definitions.contains(&(PreviewKind::Page, presentation.id.clone()))
        })
        .collect::<Vec<_>>();
    if page_presentations.is_empty() {
        return ApplicationNavigationGraph::default();
    }

    let mut inputs = BTreeSet::new();
    for presentation in &page_presentations {
        collect_presentation_inputs(&presentation.nodes, machine.local_input.id(), &mut inputs);
    }
    let mut targets_by_input = BTreeMap::<String, BTreeSet<String>>::new();
    for input in inputs {
        let Some(handler) = machine.handlers.get(&input) else {
            continue;
        };
        let mut routes = BTreeSet::new();
        collect_handler_route_constructors(
            &handler.body,
            program,
            machine,
            port,
            route_type,
            &mut BTreeMap::new(),
            &mut BTreeSet::new(),
            &mut routes,
        );
        if !routes.is_empty() {
            targets_by_input.insert(input, routes);
        }
    }
    if targets_by_input.is_empty() {
        return ApplicationNavigationGraph::default();
    }

    // The evidence module role permits an example to omit an explicit
    // presentation. Such examples are still complete, honest application
    // states. When the route value is unambiguous, use the concrete preview
    // as the graph node so a single UI declaration (A0's ReturnDeskWeb) can
    // expose several logical locations without pretending they are several
    // declarations.
    let page_subjects = page_presentations
        .iter()
        .map(|presentation| presentation.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut states = Vec::<(&Preview, String)>::new();
    for preview in previews.iter().filter(|preview| {
        preview.identity.kind == PreviewKind::Page
            && page_subjects.contains(preview.identity.subject.as_str())
    }) {
        let Some(evidence) = preview.evidence.as_ref() else {
            continue;
        };
        let Ok(observation) = Value::from_wire_json(&evidence.observation) else {
            continue;
        };
        let mut constructors = BTreeSet::new();
        collect_value_constructors(&observation, route_type, &mut constructors);
        if constructors.len() == 1 {
            states.push((
                preview,
                constructors
                    .pop_first()
                    .expect("a single route constructor exists"),
            ));
        }
    }
    if states.is_empty() {
        return ApplicationNavigationGraph::default();
    }
    states.sort_by(|(left, left_route), (right, right_route)| {
        logical_route_preview_node(left)
            .cmp(&logical_route_preview_node(right))
            .then_with(|| left_route.cmp(right_route))
    });

    let mut graph = ApplicationNavigationGraph::default();
    let mut target_by_route = BTreeMap::<String, &Preview>::new();
    for (preview, route) in &states {
        graph.nodes.push(InteractionNode {
            id: logical_route_preview_node(preview),
            kind: NodeKind::Page,
            label: format!(
                "{} / {} / {route}",
                preview.identity.subject, preview.identity.example
            ),
            modality: None,
        });
        match target_by_route.entry(route.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(preview);
            }
            std::collections::btree_map::Entry::Occupied(mut entry)
                if preview.is_default && !entry.get().is_default =>
            {
                entry.insert(preview);
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }

    for (source, _) in &states {
        for (input, routes) in &targets_by_input {
            for route in routes {
                let Some(target) = target_by_route.get(route) else {
                    continue;
                };
                let mut edge = application_edge(
                    ApplicationEdgeKind::Navigate,
                    logical_route_preview_node(source),
                    logical_route_preview_node(target),
                    input.clone(),
                );
                edge.command = Some(format!("{port}.{route}"));
                graph.edges.push(edge);
            }
        }
    }
    graph
}

fn logical_route_preview_node(preview: &Preview) -> String {
    format!("preview:{}", preview.id)
}

fn collect_value_constructors(value: &Value, type_id: &str, output: &mut BTreeSet<String>) {
    match value {
        Value::Variant {
            type_id: candidate,
            constructor,
            fields,
        } => {
            if candidate == type_id {
                output.insert(constructor.clone());
            }
            for (_, value) in fields {
                collect_value_constructors(value, type_id, output);
            }
        }
        Value::Key { value, .. } => collect_value_constructors(value, type_id, output),
        Value::Tuple(values)
        | Value::Seq(values)
        | Value::NonEmpty(values)
        | Value::Set(values) => {
            for value in values {
                collect_value_constructors(value, type_id, output);
            }
        }
        Value::Record(fields) => {
            for (_, value) in fields {
                collect_value_constructors(value, type_id, output);
            }
        }
        Value::Map(entries) => {
            for (key, value) in entries {
                collect_value_constructors(key, type_id, output);
                collect_value_constructors(value, type_id, output);
            }
        }
        Value::Table { entries, .. } => {
            for (_, value) in entries {
                collect_value_constructors(value, type_id, output);
            }
        }
        Value::Unit
        | Value::Bool(_)
        | Value::Integer { .. }
        | Value::Decimal(_)
        | Value::Ratio(_)
        | Value::Boundary(_)
        | Value::Text(_) => {}
    }
}

fn collect_presentation_inputs(nodes: &[UiNode], input_type: &str, output: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            UiNode::Element {
                attributes,
                children,
                ..
            } => {
                for attribute in attributes {
                    if let UiAttributeValue::Event { input, .. } = &attribute.value {
                        collect_nominal_constructors(input, input_type, output);
                    }
                }
                collect_presentation_inputs(children, input_type, output);
            }
            UiNode::If { children, .. } | UiNode::Each { children, .. } => {
                collect_presentation_inputs(children, input_type, output);
            }
            UiNode::Match { cases, .. } => {
                for case in cases {
                    collect_presentation_inputs(&case.children, input_type, output);
                }
            }
            UiNode::Text { .. } | UiNode::Interpolation { .. } => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_handler_route_constructors(
    statements: &[Statement],
    program: &Program,
    machine: &Machine,
    port: &str,
    route_type: &str,
    locals: &mut BTreeMap<String, Expr>,
    visiting_transitions: &mut BTreeSet<String>,
    output: &mut BTreeSet<String>,
) {
    for statement in statements {
        match statement {
            Statement::Let { name, value, .. } => {
                locals.insert(name.clone(), value.clone());
            }
            Statement::Emit { value, .. } => {
                let Expr::Constructor {
                    constructor,
                    fields,
                    ..
                } = value
                else {
                    continue;
                };
                if constructor != &format!("{port}.push")
                    && constructor != &format!("{port}.replace")
                {
                    continue;
                }
                let Some((_, location)) = fields
                    .iter()
                    .find(|(name, _)| name.as_deref() == Some("location"))
                    .or_else(|| fields.first())
                else {
                    continue;
                };
                collect_route_constructors(
                    location,
                    route_type,
                    program,
                    machine,
                    locals,
                    &mut BTreeSet::new(),
                    output,
                );
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                collect_handler_route_constructors(
                    then_body,
                    program,
                    machine,
                    port,
                    route_type,
                    &mut locals.clone(),
                    visiting_transitions,
                    output,
                );
                collect_handler_route_constructors(
                    else_body,
                    program,
                    machine,
                    port,
                    route_type,
                    &mut locals.clone(),
                    visiting_transitions,
                    output,
                );
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_handler_route_constructors(
                        &arm.body,
                        program,
                        machine,
                        port,
                        route_type,
                        &mut locals.clone(),
                        visiting_transitions,
                        output,
                    );
                }
            }
            Statement::While { body, .. } => collect_handler_route_constructors(
                body,
                program,
                machine,
                port,
                route_type,
                &mut locals.clone(),
                visiting_transitions,
                output,
            ),
            Statement::Delegate { transition, .. } => {
                if visiting_transitions.insert(transition.clone()) {
                    if let Some(transition) = machine.transitions.get(transition) {
                        collect_handler_route_constructors(
                            &transition.body,
                            program,
                            machine,
                            port,
                            route_type,
                            &mut BTreeMap::new(),
                            visiting_transitions,
                            output,
                        );
                    }
                    visiting_transitions.remove(transition);
                }
            }
            Statement::Set { .. } | Statement::Finish { .. } | Statement::Unreachable { .. } => {}
        }
    }
}

fn collect_route_constructors(
    expression: &Expr,
    route_type: &str,
    program: &Program,
    machine: &Machine,
    locals: &BTreeMap<String, Expr>,
    visiting_functions: &mut BTreeSet<String>,
    output: &mut BTreeSet<String>,
) {
    if let Expr::Name { name } = expression {
        if let Some(value) = locals.get(name) {
            collect_route_constructors(
                value,
                route_type,
                program,
                machine,
                locals,
                visiting_functions,
                output,
            );
            return;
        }
        if let Some(value) = program.machine_program.constants.get(name) {
            collect_value_constructors(value, route_type, output);
        }
    }
    collect_nominal_constructors(expression, route_type, output);
    let value = serde_json::to_value(expression).expect("checked expressions serialize");
    let mut calls = BTreeSet::new();
    collect_expression_calls(&value, &mut calls);
    for call in calls {
        if !visiting_functions.insert(call.clone()) {
            continue;
        }
        if let Some(function) = machine
            .functions
            .get(&call)
            .or_else(|| program.machine_program.functions.get(&call))
        {
            collect_route_constructors(
                &function.body,
                route_type,
                program,
                machine,
                &BTreeMap::new(),
                visiting_functions,
                output,
            );
        }
        visiting_functions.remove(&call);
    }
}

fn collect_nominal_constructors(expression: &Expr, type_id: &str, output: &mut BTreeSet<String>) {
    let value = serde_json::to_value(expression).expect("checked expressions serialize");
    collect_nominal_constructors_json(&value, type_id, output);
}

fn collect_nominal_constructors_json(
    value: &serde_json::Value,
    type_id: &str,
    output: &mut BTreeSet<String>,
) {
    match value {
        serde_json::Value::Object(object) => {
            if object.get("type_id").and_then(serde_json::Value::as_str) == Some(type_id)
                && let Some(constructor) = object
                    .get("constructor")
                    .and_then(serde_json::Value::as_str)
            {
                output.insert(constructor.to_string());
            }
            for child in object.values() {
                collect_nominal_constructors_json(child, type_id, output);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_nominal_constructors_json(child, type_id, output);
            }
        }
        _ => {}
    }
}

fn collect_expression_calls(value: &serde_json::Value, output: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(object) => {
            if object.get("kind").and_then(serde_json::Value::as_str) == Some("call")
                && let Some(function) = object.get("function").and_then(serde_json::Value::as_str)
            {
                output.insert(function.to_string());
            }
            for child in object.values() {
                collect_expression_calls(child, output);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_expression_calls(child, output);
            }
        }
        _ => {}
    }
}

fn application_surface_edges(previews: &[Preview]) -> Vec<InteractionEdge> {
    let mut surfaces_by_snapshot = BTreeMap::<String, BTreeSet<String>>::new();
    for preview in previews.iter().filter(|preview| {
        preview.identity.kind == PreviewKind::Surface && preview_contains_surface(preview)
    }) {
        if let Some(snapshot) = preview_snapshot_key(preview) {
            surfaces_by_snapshot
                .entry(snapshot)
                .or_default()
                .insert(preview.identity.subject.clone());
        }
    }

    let mut edges = Vec::new();
    for preview in previews
        .iter()
        .filter(|preview| preview.identity.kind == PreviewKind::Page)
    {
        let Some(event) = preview.replay_steps.last() else {
            continue;
        };
        let Some(snapshot) = preview_snapshot_key(preview) else {
            continue;
        };
        let Some(surfaces) = surfaces_by_snapshot.get(&snapshot) else {
            continue;
        };
        if surfaces.len() != 1 {
            continue;
        }
        let surface = surfaces
            .first()
            .expect("a single surfaced snapshot has one subject");
        edges.push(application_edge(
            ApplicationEdgeKind::Present,
            format!("page:{}", preview.identity.subject),
            format!("surface:{surface}"),
            event.clone(),
        ));
    }
    edges
}

fn preview_snapshot_key(preview: &Preview) -> Option<String> {
    preview
        .evidence
        .as_ref()
        .map(|evidence| to_canonical_json(&evidence.snapshot))
}

fn preview_contains_surface(preview: &Preview) -> bool {
    let PreviewContent::Projection(projection) = &preview.content;
    render_contains_surface(&projection.document.nodes)
}

fn render_contains_surface(nodes: &[uhura_core::RenderNode]) -> bool {
    nodes.iter().any(|node| match node {
        uhura_core::RenderNode::Element {
            surface, children, ..
        } => *surface || render_contains_surface(children),
        uhura_core::RenderNode::Text { .. } => false,
    })
}

fn projection_interactions(nodes: &[uhura_core::RenderNode], machine: &str) -> Vec<Interaction> {
    let mut interactions = Vec::new();
    collect_projection_interactions(nodes, machine, &mut interactions);
    interactions
}

fn preview_data(snapshot: &EvidenceSnapshot) -> Vec<PreviewField> {
    [
        ("configuration", snapshot.configuration.to_wire_json()),
        ("state", snapshot.state.to_wire_json()),
        ("observation", snapshot.observation.to_wire_json()),
    ]
    .into_iter()
    .map(|(name, value)| PreviewField {
        group: PreviewFieldGroup::ProvidedData,
        name: name.to_string(),
        key: None,
        value: PreviewFieldValue::ReadyJson(value),
        source: None,
    })
    .collect()
}

fn preview_replay(
    program: &Program,
    machine: &str,
    provenance: &Provenance,
    receipts: &[ReactionReceipt],
) -> (Vec<String>, Vec<serde_json::Value>) {
    let replay = receipts
        .iter()
        .map(|receipt| {
            let label = match &receipt.input {
                Value::Variant { constructor, .. } => constructor.clone(),
                Value::Unit => "unit".to_string(),
                _ => format!("reaction-{}", receipt.sequence),
            };
            let resolution = serde_json::to_value(&receipt.resolution)
                .expect("Uhura reaction resolutions are serializable");
            let dispatch = replay_dispatch(program, machine, provenance, receipt);
            (
                label.clone(),
                serde_json::json!({
                    "label": label,
                    "kind": "semantic",
                    "payload": receipt.input.to_wire_json(),
                    "dispatch": dispatch,
                    "effects": {
                        "writes": [{
                            "preStateHash": receipt.pre_state_hash,
                            "postStateHash": receipt.post_state_hash,
                        }],
                        "commands": receipt.ordered_commands.iter().map(Value::to_wire_json).collect::<Vec<_>>(),
                        "intents": [resolution],
                        "structural": [],
                        "projections": [{
                            "observation": receipt.post_observation.to_wire_json(),
                        }],
                    },
                }),
            )
        })
        .collect::<Vec<_>>();
    replay.into_iter().unzip()
}

fn replay_dispatch(
    program: &Program,
    machine: &str,
    provenance: &Provenance,
    receipt: &ReactionReceipt,
) -> serde_json::Value {
    let Value::Variant { constructor, .. } = &receipt.input else {
        return serde_json::Value::Null;
    };
    let Some(machine) = program.machine_program.machines.get(machine) else {
        return serde_json::Value::Null;
    };
    let Some(_handler) = machine.handlers.get(constructor) else {
        return serde_json::Value::Null;
    };
    let selected = machine
        .handlers
        .keys()
        .position(|input| input == constructor)
        .expect("a selected checked handler occurs in canonical handler order");
    // A composed Part is authored provenance inside one aggregate machine,
    // never a runtime actor or child instance. Keep its qualified authored
    // input (`notice_controls.DismissNotice`) while dispatch identity remains
    // the actual aggregate machine and receipt instance.
    let authored_input = replay_authored_input(&machine.id, constructor, provenance);
    serde_json::json!({
        "scope": receipt.instance,
        "definition": machine.id,
        "on": authored_input,
        "guards": [{
            "handler": selected,
            "result": "satisfied",
        }],
        "selected": selected,
        "aborted": serde_json::Value::Null,
    })
}

fn replay_authored_input(machine_id: &str, input: &str, provenance: &Provenance) -> String {
    let owner = replay_handler_owner(machine_id, input, provenance);
    if owner == "root" {
        return input.to_string();
    }
    let selector = input
        .rsplit_once('.')
        .map_or(input, |(_, selector)| selector);
    format!("{owner}.{selector}")
}

fn replay_handler_owner(machine_id: &str, input: &str, provenance: &Provenance) -> String {
    let mut candidates = vec![("root", input)];
    if let Some((owner, selector)) = input.rsplit_once('.') {
        candidates.insert(0, (owner, selector));
    }
    for (owner, selector) in candidates {
        let node = semantic_node_id(
            machine_id,
            owner,
            "handler",
            &format!("handler/{selector}/0"),
        );
        if provenance
            .occurrences
            .iter()
            .any(|occurrence| occurrence.node == node && occurrence.owner == owner)
        {
            return owner.to_string();
        }
    }
    "root".into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HistorySegment {
    scenario: String,
    upper_sequence: u64,
    boundary_pin: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LineageCandidate {
    preview_index: usize,
    segment_index: usize,
    next_sequence: u64,
}

fn pin_artifact<'a>(
    evidence: &'a EvidenceReport,
    reference: &EvidenceRef,
) -> Option<&'a uhura_core::EvidencePinArtifact> {
    evidence
        .artifacts
        .pins
        .get(&format!("{}::{}", reference.scenario, reference.pin))
}

fn preview_reference(preview: &Preview) -> Option<EvidenceRef> {
    preview.evidence.as_ref().map(|evidence| EvidenceRef {
        scenario: evidence.scenario.clone(),
        pin: evidence.pin.clone(),
    })
}

/// Resolve the linear evidence history ending at one published pin.
///
/// Segments are ordered child-first. A snapshot-origin hop is admitted only
/// when its checked reference still resolves to the exact published pin. This
/// lets Editor lineage cross scenario ids without guessing through missing or
/// ambiguous evidence.
fn history_segments(
    program: &Program,
    evidence: &EvidenceReport,
    reference: &EvidenceRef,
    snapshot: &EvidenceSnapshot,
) -> Vec<HistorySegment> {
    let mut segments = Vec::new();
    let mut scenario = reference.scenario.clone();
    let mut upper_sequence = snapshot.next_sequence;
    let mut boundary_pin = None;
    let mut visited = BTreeSet::new();

    loop {
        if !visited.insert((scenario.clone(), upper_sequence, boundary_pin.clone())) {
            break;
        }
        segments.push(HistorySegment {
            scenario: scenario.clone(),
            upper_sequence,
            boundary_pin: boundary_pin.clone(),
        });
        let Some(declaration) = program.evidence.scenarios.get(&scenario) else {
            break;
        };
        let ScenarioOrigin::Snapshot { reference: origin } = &declaration.origin else {
            break;
        };
        let Some(pin) = pin_artifact(evidence, origin) else {
            break;
        };
        scenario = origin.scenario.clone();
        upper_sequence = pin.snapshot.next_sequence;
        boundary_pin = Some(origin.pin.clone());
    }

    segments
}

fn lineage_candidate(
    segments: &[HistorySegment],
    candidate: &EvidenceRef,
    next_sequence: u64,
    preview_index: usize,
) -> Option<LineageCandidate> {
    segments
        .iter()
        .enumerate()
        .find_map(|(segment_index, segment)| {
            if segment.scenario != candidate.scenario {
                return None;
            }
            let precedes_boundary = next_sequence < segment.upper_sequence;
            let is_exact_origin = next_sequence == segment.upper_sequence
                && segment.boundary_pin.as_deref() == Some(candidate.pin.as_str());
            (precedes_boundary || is_exact_origin).then_some(LineageCandidate {
                preview_index,
                segment_index,
                next_sequence,
            })
        })
}

/// Return the exact receipt interval from a known ancestor pin to the child.
///
/// Each segment must be complete and contiguous. If a report or receipt is
/// missing, the caller declines the connector instead of presenting a
/// fabricated replay edge.
fn receipt_delta(
    evidence: &EvidenceReport,
    segments: &[HistorySegment],
    parent_segment: usize,
    parent_sequence: u64,
) -> Option<Vec<ReactionReceipt>> {
    let mut delta = Vec::new();
    for segment_index in (0..=parent_segment).rev() {
        let segment = &segments[segment_index];
        let lower_sequence = if segment_index == parent_segment {
            parent_sequence
        } else {
            segments.get(segment_index + 1)?.upper_sequence
        };
        let expected_count = segment.upper_sequence.checked_sub(lower_sequence)?;
        let report = evidence
            .scenarios
            .iter()
            .find(|report| report.scenario == segment.scenario)?;
        let receipts = report
            .receipts
            .iter()
            .filter(|receipt| {
                lower_sequence <= receipt.sequence && receipt.sequence < segment.upper_sequence
            })
            .collect::<Vec<_>>();
        if u64::try_from(receipts.len()).ok()? != expected_count
            || receipts.iter().enumerate().any(|(offset, receipt)| {
                receipt.sequence
                    != lower_sequence
                        + u64::try_from(offset).expect("a receipt offset always fits in u64")
            })
        {
            return None;
        }
        delta.extend(receipts.into_iter().cloned());
    }
    Some(delta)
}

fn pin_receipt_log(
    evidence: &EvidenceReport,
    scenario: &str,
    snapshot: &EvidenceSnapshot,
) -> Option<serde_json::Value> {
    evidence
        .scenarios
        .iter()
        .find(|report| report.scenario == scenario)
        .map(|report| {
            let receipts = report
                .receipts
                .iter()
                .filter(|receipt| receipt.sequence < snapshot.next_sequence)
                .collect::<Vec<_>>();
            let mut prefix = serde_json::json!({
                "scenario": report.scenario,
                "machine": report.machine,
                "nextSequence": snapshot.next_sequence,
                "receipts": receipts,
            });
            exactify_sequences(&mut prefix);
            prefix
        })
}

fn establish_preview_lineage(
    program: &Program,
    evidence: &EvidenceReport,
    provenance: &Provenance,
    previews: &mut [Preview],
) {
    let references = previews.iter().map(preview_reference).collect::<Vec<_>>();
    let mut updates = Vec::new();

    for (child_index, child) in previews.iter().enumerate() {
        let Some(child_reference) = &references[child_index] else {
            continue;
        };
        let Some(child_pin) = pin_artifact(evidence, child_reference) else {
            continue;
        };
        let segments = history_segments(program, evidence, child_reference, &child_pin.snapshot);
        let mut candidates = previews
            .iter()
            .enumerate()
            .filter(|(index, candidate)| {
                *index != child_index
                    && candidate.identity.kind == child.identity.kind
                    && candidate.identity.subject == child.identity.subject
            })
            .filter_map(|(index, _)| {
                let reference = references[index].as_ref()?;
                let pin = pin_artifact(evidence, reference)?;
                if pin.snapshot.machine != child_pin.snapshot.machine
                    || pin.snapshot.instance != child_pin.snapshot.instance
                {
                    return None;
                }
                lineage_candidate(&segments, reference, pin.snapshot.next_sequence, index)
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            left.segment_index
                .cmp(&right.segment_index)
                .then_with(|| right.next_sequence.cmp(&left.next_sequence))
                .then_with(|| {
                    previews[left.preview_index]
                        .identity
                        .example
                        .cmp(&previews[right.preview_index].identity.example)
                })
        });
        let Some(parent) = candidates.first() else {
            continue;
        };
        if candidates.get(1).is_some_and(|other| {
            other.segment_index == parent.segment_index
                && other.next_sequence == parent.next_sequence
        }) {
            continue;
        }
        let Some(receipts) = receipt_delta(
            evidence,
            &segments,
            parent.segment_index,
            parent.next_sequence,
        ) else {
            continue;
        };
        let (replay_steps, replay) =
            preview_replay(program, &child_pin.snapshot.machine, provenance, &receipts);
        updates.push((
            child_index,
            previews[parent.preview_index].identity.example.clone(),
            replay_steps,
            replay,
        ));
    }

    for (child_index, parent_example, replay_steps, replay) in updates {
        let child = &mut previews[child_index];
        child.from = Some(parent_example);
        child.derived = true;
        child.replay_steps = replay_steps;
        child.replay = replay;
    }
}

fn collect_projection_interactions(
    nodes: &[uhura_core::RenderNode],
    machine: &str,
    interactions: &mut Vec<Interaction>,
) {
    for node in nodes {
        let uhura_core::RenderNode::Element {
            key,
            element,
            events,
            children,
            ..
        } = node
        else {
            continue;
        };
        interactions.extend(events.iter().map(|event| Interaction {
            node_key: key.clone(),
            element: element.clone(),
            kind: InteractionKind::Input,
            event: event.event.clone(),
            emit: event.binding.clone(),
            scope: machine.to_string(),
            payload: serde_json::Value::Null,
            carries: BTreeMap::new(),
        }));
        collect_projection_interactions(children, machine, interactions);
    }
}

fn instance_from_snapshot(
    program: &Program,
    snapshot: &EvidenceSnapshot,
) -> Result<Instance, String> {
    let instance = program
        .machine_program
        .restore(&Checkpoint {
            protocol: CHECKPOINT_PROTOCOL.into(),
            instance: snapshot.instance.clone(),
            machine: snapshot.machine.clone(),
            machine_program_hash: snapshot.machine_program_hash.clone(),
            configuration: snapshot.configuration.clone(),
            state: snapshot.state.clone(),
            inbox: snapshot.inbox.clone(),
            lifecycle: snapshot.lifecycle,
            next_sequence: snapshot.next_sequence,
            trace_prefix_hash: snapshot.trace_prefix_hash.clone(),
        })
        .map_err(|error| error.to_string())?;
    if instance.observation != snapshot.observation {
        return Err("restored observation differs from the evidence snapshot".into());
    }
    Ok(instance)
}

fn exactify_sequences(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                exactify_sequences(value);
            }
        }
        serde_json::Value::Object(fields) => {
            for (name, value) in fields {
                if matches!(name.as_str(), "sequence" | "next_sequence" | "nextSequence")
                    && let Some(number) = value.as_u64()
                {
                    *value = serde_json::Value::String(number.to_string());
                } else {
                    exactify_sequences(value);
                }
            }
        }
        _ => {}
    }
}

/// Listenerless Editor/Play host. The state and event hubs have host-session
/// lifetime; individual valid Play generations are immutable values inside it.
pub struct Host {
    state: RwLock<DevState>,
    play_clients: Clients,
    editor_clients: Clients,
    web: Arc<WebAssets>,
}

impl Host {
    /// Publish revision 1 and create stable route/event state.
    pub fn new(
        web: WebAssets,
        candidate: ClientCandidate,
    ) -> Result<(Self, PublicationReport), String> {
        if candidate.revision != 1 {
            return Err(format!(
                "initial Uhura candidate must be revision 1, got {}",
                candidate.revision
            ));
        }
        let summary = candidate.summary();
        let editor = EditorHostState::initial(candidate.editor)?;
        let mut play = PlayState::default();
        apply_play(&mut play, candidate.play);
        let report = publication_report(&play, summary);
        let event_admission = Arc::new(EventAdmission::new(MAX_EVENT_STREAMS_PER_HOST));
        Ok((
            Self {
                state: RwLock::new(DevState { play, editor }),
                play_clients: Arc::new(Mutex::new(ClientRegistry::with_admission(Arc::clone(
                    &event_admission,
                )))),
                editor_clients: Arc::new(Mutex::new(ClientRegistry::with_admission(
                    event_admission,
                ))),
                web: Arc::new(web),
            },
            report,
        ))
    }

    /// Atomically replace Editor publication state and advance Play's
    /// last-good state, then notify the stable event hubs.
    pub fn publish(&self, candidate: ClientCandidate) -> Result<PublicationReport, String> {
        let summary = candidate.summary();
        let revision = candidate.revision;
        let (report, editor_payload, play_payload) = {
            let mut state = self.state.write().expect("state lock");
            state.editor.apply(revision, candidate.editor)?;
            apply_play(&mut state.play, candidate.play);
            (
                publication_report(&state.play, summary),
                editor_sse_payload(revision),
                play_sse_payload(&state.play),
            )
        };
        broadcast(&self.editor_clients, &editor_payload);
        broadcast(&self.play_clients, &play_payload);
        Ok(report)
    }

    pub fn source_revision(&self) -> u64 {
        self.state
            .read()
            .expect("state lock")
            .editor
            .source_revision
    }
}

fn publication_report(play: &PlayState, summary: CandidateSummary) -> PublicationReport {
    PublicationReport {
        source_revision: summary.revision,
        editor_current: summary.editor_current,
        preview_count: summary.preview_count,
        replay_derived_count: summary.replay_derived_count,
        play_generation: play.generation,
        play_ok: play.ok,
        has_good_play: play.good.is_some(),
    }
}

// ── SSE ────────────────────────────────────────────────────────────────────

// Event frames are invalidations, not an artifact log: Editor and Play clients
// refetch the host's current immutable state after receiving one. Each
// subscriber therefore has a one-frame queue. If it is stalled, the already
// queued frame remains a sufficient invalidation and later publications are
// intentionally coalesced instead of growing memory without bound.
const BLOCKING_EVENT_STREAM_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(1);
// `tiny_http` writes unknown-length HTTP/1.1 bodies through an 8 KiB chunk
// encoder that does not flush partial chunks. A keepalive must cross that
// boundary so every timeout reaches the socket instead of accumulating in the
// encoder while a disconnected client continues occupying its worker.
const BLOCKING_EVENT_STREAM_WRITE_BOUNDARY: usize = 8 * 1024;
// Editor and Play share this host-session budget. Keeping admission below the
// HTTP adapters means every listener implementation gets the same bound and a
// disconnected response releases capacity when its `EventStream` is dropped.
const MAX_EVENT_STREAMS_PER_HOST: usize = 4;

struct EventAdmission {
    active: Mutex<usize>,
    limit: usize,
}

impl EventAdmission {
    fn new(limit: usize) -> Self {
        Self {
            active: Mutex::new(0),
            limit,
        }
    }

    fn try_acquire(self: &Arc<Self>) -> Option<EventStreamPermit> {
        let mut active = self.active.lock().expect("event admission lock");
        if *active >= self.limit {
            return None;
        }
        *active += 1;
        Some(EventStreamPermit {
            admission: Arc::clone(self),
        })
    }
}

struct EventStreamPermit {
    admission: Arc<EventAdmission>,
}

impl Drop for EventStreamPermit {
    fn drop(&mut self) {
        let mut active = self.admission.active.lock().expect("event admission lock");
        *active = (*active)
            .checked_sub(1)
            .expect("event admission count underflow");
    }
}

struct ClientRegistry {
    next_id: u64,
    clients: BTreeMap<u64, SyncSender<String>>,
    admission: Arc<EventAdmission>,
}

impl ClientRegistry {
    fn with_admission(admission: Arc<EventAdmission>) -> Self {
        Self {
            next_id: 0,
            clients: BTreeMap::new(),
            admission,
        }
    }
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::with_admission(Arc::new(EventAdmission::new(MAX_EVENT_STREAMS_PER_HOST)))
    }
}

type Clients = Arc<Mutex<ClientRegistry>>;

fn play_sse_payload(play: &PlayState) -> String {
    let mut event = serde_json::json!({
        "generation": play.generation,
        "ok": play.ok,
    });
    if let Some(diagnostics) = &play.diagnostics {
        event["diagnostics"] = diagnostics.clone();
    }
    sse_frame(&event)
}

fn editor_sse_payload(source_revision: u64) -> String {
    sse_frame(&serde_json::json!({
        "protocol": EDITOR_EVENT_PROTOCOL,
        "sourceRevision": source_revision,
    }))
}

fn sse_frame(value: &serde_json::Value) -> String {
    // Padding crosses common streaming response buffers; EventSource ignores
    // comments.
    format!(
        "data: {}\n\n: {}\n\n",
        to_canonical_json(value),
        "·".repeat(4096)
    )
}

fn broadcast(clients: &Clients, payload: &str) {
    let mut clients = clients.lock().expect("clients lock");
    clients
        .clients
        .retain(|_, sender| match sender.try_send(payload.to_string()) {
            Ok(()) | Err(TrySendError::Full(_)) => true,
            Err(TrySendError::Disconnected(_)) => false,
        });
}

pub struct EventStream {
    receiver: Receiver<String>,
    buffer: Vec<u8>,
    offset: usize,
    subscription_id: u64,
    clients: Weak<Mutex<ClientRegistry>>,
    blocking_keepalive_interval: Duration,
    _admission_permit: Option<EventStreamPermit>,
}

/// One bounded wait on a host-session event stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventStreamPoll {
    Frame(String),
    Timeout,
    Closed,
}

impl EventStream {
    /// Poll once without occupying an executor thread.
    pub fn try_next_frame(&self) -> EventStreamPoll {
        match self.receiver.try_recv() {
            Ok(frame) => EventStreamPoll::Frame(frame),
            Err(TryRecvError::Empty) => EventStreamPoll::Timeout,
            Err(TryRecvError::Disconnected) => EventStreamPoll::Closed,
        }
    }

    /// Wait at most `timeout` for one complete SSE frame.
    ///
    /// Async adapters can repeat this bounded wait and drop the stream when
    /// their client disconnects, avoiding an indefinitely blocked bridge
    /// thread. Use either this framed API or [`Read`], but do not mix them on
    /// one stream.
    pub fn next_frame_timeout(&self, timeout: Duration) -> EventStreamPoll {
        match self.receiver.recv_timeout(timeout) {
            Ok(frame) => EventStreamPoll::Frame(frame),
            Err(RecvTimeoutError::Timeout) => EventStreamPoll::Timeout,
            Err(RecvTimeoutError::Disconnected) => EventStreamPoll::Closed,
        }
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        if let Some(clients) = self.clients.upgrade() {
            clients
                .lock()
                .expect("clients lock")
                .clients
                .remove(&self.subscription_id);
        }
    }
}

impl Read for EventStream {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.offset >= self.buffer.len() {
            match self.receiver.recv_timeout(self.blocking_keepalive_interval) {
                Ok(frame) => {
                    self.buffer = frame.into_bytes();
                }
                Err(RecvTimeoutError::Timeout) => {
                    // The blocking `Read` adapter cannot otherwise observe a
                    // quiet client disconnect: its next operation would remain
                    // parked on the event channel. An SSE comment is invisible
                    // to EventSource while giving the HTTP writer a bounded
                    // opportunity to discover a closed socket.
                    self.buffer.clear();
                    self.buffer.push(b':');
                    self.buffer
                        .resize(BLOCKING_EVENT_STREAM_WRITE_BOUNDARY + 1, b' ');
                    self.buffer.extend_from_slice(b"\n\n");
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
            self.offset = 0;
        }
        let count = (self.buffer.len() - self.offset).min(output.len());
        output[..count].copy_from_slice(&self.buffer[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

fn subscribe(clients: &Clients, hello: impl FnOnce() -> String) -> Option<EventStream> {
    subscribe_with_blocking_keepalive(clients, hello, BLOCKING_EVENT_STREAM_KEEPALIVE_INTERVAL)
}

fn subscribe_with_blocking_keepalive(
    clients: &Clients,
    hello: impl FnOnce() -> String,
    blocking_keepalive_interval: Duration,
) -> Option<EventStream> {
    let admission = Arc::clone(&clients.lock().expect("clients lock").admission);
    let admission_permit = admission.try_acquire()?;
    let (sender, receiver) = sync_channel::<String>(1);
    let subscription_id = {
        // Registration and snapshot share the broadcast lock: an update can
        // be coalesced with the initial invalidation but can never leave the
        // subscriber without an invalidation to refetch current state.
        let mut clients = clients.lock().expect("clients lock");
        sender
            .try_send(hello())
            .expect("new event queue has one available slot");
        let subscription_id = clients.next_id;
        clients.next_id += 1;
        clients.clients.insert(subscription_id, sender);
        subscription_id
    };
    Some(EventStream {
        receiver,
        buffer: Vec::new(),
        offset: 0,
        subscription_id,
        clients: Arc::downgrade(clients),
        blocking_keepalive_interval,
        _admission_permit: Some(admission_permit),
    })
}

// ── one web application and namespaced transport ───────────────────────────

#[derive(Clone, Debug)]
pub struct WebAssets {
    files: Arc<BTreeMap<String, WebFile>>,
    index: Arc<Vec<u8>>,
    wasm_files: Arc<BTreeMap<String, WebFile>>,
}

/// One immutable file captured in a [`WebAssets`] snapshot.
///
/// Paths are manifest-relative and begin with `web/` or `wasm/`. Aggregate
/// hosts can compare this inventory with a package manifest without rereading
/// mutable filesystem state after the served bytes have been captured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebAssetDigest {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug)]
struct WebFile {
    bytes: Arc<Vec<u8>>,
    content_type: String,
}

impl WebAssets {
    /// Snapshot explicit frontend and Wasm directories into an immutable host
    /// value. Aggregate hosts should use this constructor with package-owned
    /// paths instead of relying on standalone CLI discovery.
    pub fn from_directories(web_root: &Path, wasm_root: &Path) -> Result<Self, String> {
        load_web_assets(web_root, Some(wasm_root))
    }

    /// Snapshot a frontend directory without a Wasm bundle. This preserves
    /// the standalone CLI's useful pre-Wasm diagnostics; packaged aggregate
    /// hosts should use [`Self::from_directories`] instead.
    pub fn from_frontend_directory(web_root: &Path) -> Result<Self, String> {
        load_web_assets(web_root, None)
    }

    /// Describe the exact immutable bytes held by this snapshot.
    #[must_use]
    pub fn inventory(&self) -> Vec<WebAssetDigest> {
        let mut inventory = Vec::with_capacity(self.files.len() + self.wasm_files.len());
        inventory.extend(self.files.iter().map(|(path, file)| WebAssetDigest {
            path: format!("web/{path}"),
            sha256: sha256_hex(file.bytes.as_slice()),
            size: file.bytes.len() as u64,
        }));
        inventory.extend(self.wasm_files.iter().map(|(path, file)| WebAssetDigest {
            path: format!("wasm/{path}"),
            sha256: sha256_hex(file.bytes.as_slice()),
            size: file.bytes.len() as u64,
        }));
        inventory.sort_by(|left, right| left.path.cmp(&right.path));
        inventory
    }
}

#[cfg(test)]
fn load_web_app_from(candidates: &[PathBuf]) -> Result<WebAssets, String> {
    let mut attempted = Vec::new();
    for root in candidates {
        if attempted.iter().any(|seen: &PathBuf| seen == root) {
            continue;
        }
        attempted.push(root.clone());
        let index_path = root.join("index.html");
        match std::fs::symlink_metadata(&index_path) {
            Ok(_) => {
                return WebAssets::from_frontend_directory(root);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("could not read {}: {error}", index_path.display()));
            }
        }
    }
    let locations = attempted
        .iter()
        .map(|root| root.join("index.html").display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "browser application is not built (looked for {locations}); set \
         UHURA_WEB_DIST or build web/ before starting a browser surface"
    ))
}

fn load_web_assets(
    web_root: &Path,
    explicit_wasm_root: Option<&Path>,
) -> Result<WebAssets, String> {
    let index_path = web_root.join("index.html");
    let files = snapshot_web_bundle(web_root)?;
    let index = files
        .get("index.html")
        .ok_or_else(|| format!("{} is not a regular file", index_path.display()))?;
    if index.bytes.is_empty() {
        return Err(format!("{} is empty", index_path.display()));
    }
    if files.len() == 1 {
        return Err(format!(
            "browser application bundle at {} contains only index.html",
            web_root.display()
        ));
    }
    validate_index_assets(web_root, index.bytes.as_slice(), &files)?;
    let index = Arc::clone(&index.bytes);

    let wasm_files = match explicit_wasm_root {
        Some(wasm_root) => snapshot_web_bundle(wasm_root)?,
        None => BTreeMap::new(),
    };
    Ok(WebAssets {
        files: Arc::new(files),
        index,
        wasm_files: Arc::new(wasm_files),
    })
}

fn snapshot_web_bundle(root: &Path) -> Result<BTreeMap<String, WebFile>, String> {
    let mut files = BTreeMap::new();
    let mut directories = vec![root.to_path_buf()];

    while let Some(directory) = directories.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
        let mut entries = entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);

        for entry in entries {
            let path = entry.path();
            let relative = normalized_web_bundle_path(root, &path)?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
            if file_type.is_dir() {
                directories.push(path);
                continue;
            }
            if !file_type.is_file() {
                return Err(format!(
                    "browser application bundle contains an unsafe non-regular entry: {}",
                    path.display()
                ));
            }

            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            let bytes = std::fs::read(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            files.insert(
                relative,
                WebFile {
                    bytes: Arc::new(bytes),
                    content_type: content_type(extension),
                },
            );
        }
    }

    Ok(files)
}

fn validate_index_assets(
    root: &Path,
    index: &[u8],
    files: &BTreeMap<String, WebFile>,
) -> Result<(), String> {
    let index = std::str::from_utf8(index).map_err(|error| {
        format!(
            "{} is not UTF-8: {error}",
            root.join("index.html").display()
        )
    })?;
    let references = index_asset_references(index)?;
    if references.is_empty() {
        return Err(format!(
            "{} references no local JavaScript or CSS assets",
            root.join("index.html").display()
        ));
    }
    for reference in references {
        if !files.contains_key(&reference) {
            return Err(format!(
                "{} references a missing application asset: /{reference}",
                root.join("index.html").display()
            ));
        }
    }
    Ok(())
}

fn index_asset_references(index: &str) -> Result<BTreeSet<String>, String> {
    let bytes = index.as_bytes();
    let mut references = BTreeSet::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(offset) = bytes[cursor..].iter().position(|byte| *byte == b'<') else {
            break;
        };
        let tag_start = cursor + offset;
        if bytes[tag_start..].starts_with(b"<!--") {
            cursor = bytes[tag_start + 4..]
                .windows(3)
                .position(|window| window == b"-->")
                .map_or(bytes.len(), |offset| tag_start + 4 + offset + 3);
            continue;
        }

        let name_start = tag_start + 1;
        let Some(first) = bytes.get(name_start) else {
            break;
        };
        if matches!(first, b'/' | b'!' | b'?') {
            cursor = html_tag_end(bytes, name_start).map_or(bytes.len(), |tag_end| tag_end + 1);
            continue;
        }
        if !first.is_ascii_alphabetic() {
            cursor = name_start;
            continue;
        }

        let mut name_end = name_start;
        while bytes
            .get(name_end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
        {
            name_end += 1;
        }
        let Some(tag_end) = html_tag_end(bytes, name_end) else {
            break;
        };
        collect_index_asset_attributes(index, name_end, tag_end, &mut references)?;
        cursor = tag_end + 1;

        let name = &index[name_start..name_end];
        if is_html_raw_text_element(name) {
            cursor = skip_html_raw_text(index, cursor, name);
        }
    }
    Ok(references)
}

fn html_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in bytes[start..].iter().copied().enumerate() {
        if let Some(expected) = quote {
            if byte == expected {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'>' => return Some(start + offset),
            _ => {}
        }
    }
    None
}

fn collect_index_asset_attributes(
    index: &str,
    start: usize,
    end: usize,
    references: &mut BTreeSet<String>,
) -> Result<(), String> {
    let bytes = index.as_bytes();
    let mut cursor = start;
    while cursor < end {
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'/')
        {
            cursor += 1;
        }
        if cursor >= end {
            break;
        }

        let name_start = cursor;
        while bytes.get(cursor).is_some_and(|byte| {
            !byte.is_ascii_whitespace()
                && !matches!(byte, b'\0' | b'\'' | b'"' | b'/' | b'=' | b'>')
        }) {
            cursor += 1;
        }
        if cursor == name_start {
            cursor += 1;
            continue;
        }
        let name = &index[name_start..cursor];

        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if cursor >= end || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if cursor >= end {
            break;
        }

        let (value_start, value_end) = if matches!(bytes[cursor], b'\'' | b'"') {
            let quote = bytes[cursor];
            cursor += 1;
            let value_start = cursor;
            while cursor < end && bytes[cursor] != quote {
                cursor += 1;
            }
            let value_end = cursor;
            if cursor < end {
                cursor += 1;
            }
            (value_start, value_end)
        } else {
            let value_start = cursor;
            while cursor < end && !bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            (value_start, cursor)
        };

        if name.eq_ignore_ascii_case("src") || name.eq_ignore_ascii_case("href") {
            record_index_asset_reference(&index[value_start..value_end], references)?;
        }
    }
    Ok(())
}

fn record_index_asset_reference(
    value: &str,
    references: &mut BTreeSet<String>,
) -> Result<(), String> {
    let path_end = value.find(['?', '#']).unwrap_or(value.len());
    let path = &value[..path_end];
    let lowercase = path.to_ascii_lowercase();
    if !lowercase.ends_with(".js") && !lowercase.ends_with(".mjs") && !lowercase.ends_with(".css") {
        return Ok(());
    }
    if path.starts_with("//")
        || path
            .find(':')
            .is_some_and(|colon| path.find('/').is_none_or(|slash| colon < slash))
    {
        return Ok(());
    }
    let relative = path.strip_prefix('/').unwrap_or(path);
    if relative.contains('\\')
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || Path::new(relative).is_absolute()
    {
        return Err(format!(
            "index.html contains an unsafe local application asset reference: {value}"
        ));
    }
    references.insert(relative.to_string());
    Ok(())
}

fn is_html_raw_text_element(name: &str) -> bool {
    [
        "script",
        "style",
        "textarea",
        "title",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "plaintext",
    ]
    .into_iter()
    .any(|element| name.eq_ignore_ascii_case(element))
}

fn skip_html_raw_text(index: &str, mut cursor: usize, name: &str) -> usize {
    let bytes = index.as_bytes();
    while cursor < bytes.len() {
        let Some(offset) = bytes[cursor..].iter().position(|byte| *byte == b'<') else {
            return bytes.len();
        };
        let tag_start = cursor + offset;
        let close_name_start = tag_start + 2;
        let close_name_end = close_name_start + name.len();
        let closes_element = bytes.get(tag_start + 1) == Some(&b'/')
            && bytes
                .get(close_name_start..close_name_end)
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name.as_bytes()))
            && bytes
                .get(close_name_end)
                .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>'));
        if closes_element {
            return html_tag_end(bytes, close_name_end).map_or(bytes.len(), |tag_end| tag_end + 1);
        }
        cursor = tag_start + 1;
    }
    bytes.len()
}

fn normalized_web_bundle_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "browser application bundle entry escapes {}: {}",
            root.display(),
            path.display()
        )
    })?;
    let mut segments = Vec::new();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(format!(
                "browser application bundle contains an unsafe path: {}",
                path.display()
            ));
        };
        let segment = segment.to_str().ok_or_else(|| {
            format!(
                "browser application bundle path is not UTF-8: {}",
                path.display()
            )
        })?;
        if segment.contains('\\') {
            return Err(format!(
                "browser application bundle contains an unsafe path: {}",
                path.display()
            ));
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        return Err(format!(
            "browser application bundle contains an unsafe path: {}",
            path.display()
        ));
    }
    Ok(segments.join("/"))
}

#[cfg(test)]
fn tool_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlayArtifact {
    Ir,
    Inspect,
    Stylesheet,
    Config,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ApiRoute<'a> {
    EditorState,
    EditorEvents,
    EditorIconFonts,
    EditorIconFont(&'a str),
    PlayEvents,
    PlayArtifact(PlayArtifact),
    PlayIconFonts,
    PlayIconFont(&'a str),
    PlayProvider,
    PlayAsset(&'a str),
    PlayWasm(&'a str),
    Unknown,
}

fn api_route(path: &str) -> Option<ApiRoute<'_>> {
    let route = match path {
        "/api/editor/state" => ApiRoute::EditorState,
        "/api/editor/events" => ApiRoute::EditorEvents,
        "/api/editor/icon-fonts.json" => ApiRoute::EditorIconFonts,
        "/api/play/events" => ApiRoute::PlayEvents,
        "/api/play/icon-fonts.json" => ApiRoute::PlayIconFonts,
        "/api/play/ir.json" => ApiRoute::PlayArtifact(PlayArtifact::Ir),
        "/api/play/inspect.json" => ApiRoute::PlayArtifact(PlayArtifact::Inspect),
        "/api/play/stylesheet.css" => ApiRoute::PlayArtifact(PlayArtifact::Stylesheet),
        "/api/play/config.json" => ApiRoute::PlayArtifact(PlayArtifact::Config),
        "/api/play/provider.js" => ApiRoute::PlayProvider,
        _ => {
            if let Some(relative) = path.strip_prefix("/api/editor/icon-fonts/")
                && !relative.is_empty()
            {
                ApiRoute::EditorIconFont(relative)
            } else if let Some(relative) = path.strip_prefix("/api/play/icon-fonts/")
                && !relative.is_empty()
            {
                ApiRoute::PlayIconFont(relative)
            } else if let Some(relative) = path.strip_prefix("/api/play/assets/")
                && !relative.is_empty()
            {
                ApiRoute::PlayAsset(relative)
            } else if let Some(relative) = path.strip_prefix("/api/play/wasm/")
                && !relative.is_empty()
            {
                ApiRoute::PlayWasm(relative)
            } else if path.starts_with("/api/") {
                ApiRoute::Unknown
            } else {
                return None;
            }
        }
    };
    Some(route)
}

/// (content type, body, optional Play generation) or (status, message).
type Served = Result<(String, Vec<u8>, Option<u64>), (u16, String)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestMethod {
    Get,
    Head,
    Other,
}

pub struct RouteRequest<'a> {
    pub method: RequestMethod,
    pub url: &'a str,
}

pub struct RouteResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: RouteBody,
}

pub enum RouteBody {
    Bytes(Cursor<Vec<u8>>),
    Events(EventStream),
}

impl RouteBody {
    pub fn content_length(&self) -> Option<usize> {
        match self {
            Self::Bytes(bytes) => Some(bytes.get_ref().len()),
            Self::Events(_) => None,
        }
    }
}

impl Read for RouteBody {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Bytes(bytes) => bytes.read(output),
            Self::Events(events) => events.read(output),
        }
    }
}

impl Host {
    /// Resolve one HTTP-like request without owning a listener or server loop.
    pub fn route(&self, request: RouteRequest<'_>) -> RouteResponse {
        let method = request.method;
        finalize_route_response(method, self.route_unfinalized(request))
    }

    fn route_unfinalized(&self, request: RouteRequest<'_>) -> RouteResponse {
        let (path, query) = split_request_url(request.url);
        if request.method == RequestMethod::Other {
            return byte_response(
                405,
                "text/plain; charset=utf-8",
                "the Uhura host accepts GET and HEAD only"
                    .as_bytes()
                    .to_vec(),
                vec![("Allow".to_string(), "GET, HEAD".to_string())],
            );
        }

        match api_route(path) {
            Some(ApiRoute::PlayEvents) => {
                if request.method != RequestMethod::Get {
                    return event_method_error(path);
                }
                let Some(stream) = subscribe(&self.play_clients, || {
                    play_sse_payload(&self.state.read().expect("state lock").play)
                }) else {
                    return event_capacity_error();
                };
                return event_response(stream);
            }
            Some(ApiRoute::EditorEvents) => {
                if request.method != RequestMethod::Get {
                    return event_method_error(path);
                }
                let Some(stream) = subscribe(&self.editor_clients, || {
                    let revision = self
                        .state
                        .read()
                        .expect("state lock")
                        .editor
                        .source_revision;
                    editor_sse_payload(revision)
                }) else {
                    return event_capacity_error();
                };
                return event_response(stream);
            }
            _ => {}
        }

        let outcome = match api_route(path) {
            Some(ApiRoute::EditorState) => editor_state_artifact(&self.state),
            Some(ApiRoute::EditorIconFonts) => editor_icon_font_manifest(&self.state),
            Some(ApiRoute::EditorIconFont(file)) => editor_icon_font(&self.state, file),
            Some(ApiRoute::PlayArtifact(artifact_kind)) => {
                play_artifact(&self.state, artifact_kind)
            }
            Some(ApiRoute::PlayIconFonts) => play_icon_font_manifest(&self.state),
            Some(ApiRoute::PlayIconFont(file)) => play_icon_font(&self.state, file),
            Some(ApiRoute::PlayProvider) => provider_artifact(&self.state, query),
            Some(ApiRoute::PlayAsset(relative)) => play_asset(&self.state, relative),
            Some(ApiRoute::PlayWasm(relative)) => serve_file_map(&self.web.wasm_files, relative)
                .map_err(|(status, message)| {
                    (
                        status,
                        format!("{message}\n(build the Wasm bundle first: scripts/build-wasm.sh)"),
                    )
                }),
            Some(ApiRoute::EditorEvents | ApiRoute::PlayEvents) => {
                unreachable!("returned above")
            }
            Some(ApiRoute::Unknown) => Err((404, format!("no such API endpoint: {path}"))),
            None => application_path(&self.web, path),
        };
        served_response(outcome)
    }
}

fn finalize_route_response(method: RequestMethod, mut response: RouteResponse) -> RouteResponse {
    if method == RequestMethod::Head
        && let RouteBody::Bytes(bytes) = &mut response.body
    {
        bytes.get_mut().clear();
        bytes.set_position(0);
    }
    response
}

fn served_response(outcome: Served) -> RouteResponse {
    match outcome {
        Ok((content_type, bytes, generation)) => {
            let mut headers = Vec::new();
            if let Some(generation) = generation {
                headers.push(("X-Uhura-Generation".to_string(), generation.to_string()));
            }
            byte_response(200, &content_type, bytes, headers)
        }
        Err((status, message)) => byte_response(
            status,
            "text/plain; charset=utf-8",
            message.into_bytes(),
            Vec::new(),
        ),
    }
}

fn byte_response(
    status: u16,
    content_type: &str,
    bytes: Vec<u8>,
    mut headers: Vec<(String, String)>,
) -> RouteResponse {
    headers.push(("Content-Length".to_string(), bytes.len().to_string()));
    headers.push(("Content-Type".to_string(), content_type.to_string()));
    headers.push(("Cache-Control".to_string(), "no-store".to_string()));
    RouteResponse {
        status,
        headers,
        body: RouteBody::Bytes(Cursor::new(bytes)),
    }
}

fn event_response(stream: EventStream) -> RouteResponse {
    RouteResponse {
        status: 200,
        headers: vec![
            (
                "Content-Type".to_string(),
                "text/event-stream; charset=utf-8".to_string(),
            ),
            ("Cache-Control".to_string(), "no-store".to_string()),
        ],
        body: RouteBody::Events(stream),
    }
}

fn event_method_error(path: &str) -> RouteResponse {
    byte_response(
        405,
        "text/plain; charset=utf-8",
        format!("{path} requires GET").into_bytes(),
        vec![("Allow".to_string(), "GET".to_string())],
    )
}

fn event_capacity_error() -> RouteResponse {
    byte_response(
        503,
        "text/plain; charset=utf-8",
        b"too many active Uhura event streams; retry shortly".to_vec(),
        vec![("Retry-After".to_string(), "1".to_string())],
    )
}

fn split_request_url(url: &str) -> (&str, Option<&str>) {
    url.split_once('?')
        .map_or((url, None), |(path, query)| (path, Some(query)))
}

fn editor_state_artifact(state: &RwLock<DevState>) -> Served {
    let state = state.read().expect("state lock");
    Ok((
        content_type("json"),
        state.editor.state_json.clone().into_bytes(),
        None,
    ))
}

fn editor_icon_font_manifest(state: &RwLock<DevState>) -> Served {
    let state = state.read().expect("state lock");
    let Some(renderable) = &state.editor.last_renderable else {
        return Err((
            503,
            "no renderable Editor revision has icon-font resources yet".to_string(),
        ));
    };
    let bytes = renderable.icon_fonts.as_ref().map_or_else(
        || {
            empty_icon_font_manifest(IconFontManifestVersion::Revision(
                renderable.render.revision,
            ))
        },
        |resources| {
            icon_font_manifest(
                resources,
                IconFontManifestVersion::Revision(renderable.render.revision),
                "/api/editor/icon-fonts",
            )
        },
    );
    Ok((content_type("json"), bytes, None))
}

fn editor_icon_font(state: &RwLock<DevState>, file: &str) -> Served {
    let state = state.read().expect("state lock");
    let Some(renderable) = &state.editor.last_renderable else {
        return Err((
            503,
            "no renderable Editor revision has icon-font resources yet".to_string(),
        ));
    };
    let Some(resources) = renderable.icon_fonts.as_ref() else {
        return Err((404, format!("no such icon font: {file}")));
    };
    icon_font_file(resources, file, None)
}

fn play_icon_font_manifest(state: &RwLock<DevState>) -> Served {
    let state = state.read().expect("state lock");
    let Some(good) = &state.play.good else {
        return Err((
            503,
            "no good Play build yet — fix the project diagnostics".to_string(),
        ));
    };
    let Some(resources) = good.icon_fonts.as_ref() else {
        return Ok((
            content_type("json"),
            empty_icon_font_manifest(IconFontManifestVersion::Generation(state.play.generation)),
            Some(state.play.generation),
        ));
    };
    // Play's transport generation labels the complete currently served
    // artifact set. After a rejected check that set is last-good, but it is
    // still re-published under the new generation like every other Play
    // artifact, so the browser's coherent-fetch gate sees one version.
    Ok((
        content_type("json"),
        icon_font_manifest(
            resources,
            IconFontManifestVersion::Generation(state.play.generation),
            "/api/play/icon-fonts",
        ),
        Some(state.play.generation),
    ))
}

fn play_icon_font(state: &RwLock<DevState>, file: &str) -> Served {
    let state = state.read().expect("state lock");
    let Some(good) = &state.play.good else {
        return Err((
            503,
            "no good Play build yet — fix the project diagnostics".to_string(),
        ));
    };
    let resources = require_icon_fonts(good.icon_fonts.as_ref(), "Play")?;
    icon_font_file(resources, file, Some(state.play.generation))
}

#[derive(Clone, Copy)]
enum IconFontManifestVersion {
    Revision(u64),
    Generation(u64),
}

fn require_icon_fonts<'a>(
    resources: Option<&'a IconFontResources>,
    owner: &str,
) -> Result<&'a IconFontResources, (u16, String)> {
    resources.ok_or_else(|| {
        (
            503,
            format!("the current {owner} artifact has no icon-font resources"),
        )
    })
}

fn icon_font_manifest(
    resources: &IconFontResources,
    version: IconFontManifestVersion,
    url_base: &str,
) -> Vec<u8> {
    let families = resources
        .families
        .iter()
        .map(|(name, family)| {
            let glyphs = family
                .glyphs
                .iter()
                .map(|(name, codepoint)| (name.clone(), serde_json::json!(codepoint)))
                .collect::<serde_json::Map<_, _>>();
            (
                name.clone(),
                serde_json::json!({
                    "font": format!("{url_base}/{}.woff2", family.font_hash),
                    "sha256": family.font_hash,
                    "glyphs": glyphs,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let mut manifest = serde_json::json!({
        "protocol": ICON_FONT_MANIFEST_PROTOCOL,
        "default": resources.default,
        "families": families,
    });
    match version {
        IconFontManifestVersion::Revision(revision) => manifest["revision"] = revision.into(),
        IconFontManifestVersion::Generation(generation) => {
            manifest["generation"] = generation.into();
        }
    }
    to_canonical_json(&manifest).into_bytes()
}

fn empty_icon_font_manifest(version: IconFontManifestVersion) -> Vec<u8> {
    let mut manifest = serde_json::json!({
        "protocol": ICON_FONT_MANIFEST_PROTOCOL,
        "default": serde_json::Value::Null,
        "families": {},
    });
    match version {
        IconFontManifestVersion::Revision(revision) => manifest["revision"] = revision.into(),
        IconFontManifestVersion::Generation(generation) => {
            manifest["generation"] = generation.into();
        }
    }
    to_canonical_json(&manifest).into_bytes()
}

fn icon_font_file(resources: &IconFontResources, file: &str, generation: Option<u64>) -> Served {
    let hash = parse_icon_font_file(file)?;
    let family = resources
        .families
        .values()
        .find(|family| family.font_hash == hash)
        .ok_or_else(|| (404, format!("no such icon font: {hash}")))?;
    Ok((content_type("woff2"), family.font.to_vec(), generation))
}

fn parse_icon_font_file(file: &str) -> Result<&str, (u16, String)> {
    let Some(hash) = file.strip_suffix(".woff2") else {
        return Err((
            400,
            "bad icon-font path: expected <sha256>.woff2".to_string(),
        ));
    };
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err((
            400,
            "bad icon-font path: expected a lowercase SHA-256 digest".to_string(),
        ));
    }
    Ok(hash)
}

fn play_artifact(state: &RwLock<DevState>, artifact_kind: PlayArtifact) -> Served {
    let state = state.read().expect("state lock");
    let Some(good) = &state.play.good else {
        return Err((
            503,
            "no good Play build yet — fix the project diagnostics".to_string(),
        ));
    };
    let (extension, bytes) = match artifact_kind {
        PlayArtifact::Ir => ("json", good.ir.as_bytes()),
        PlayArtifact::Inspect => ("json", good.inspect_json.as_bytes()),
        PlayArtifact::Stylesheet => ("css", good.stylesheet.as_bytes()),
        PlayArtifact::Config => ("json", good.config_json.as_bytes()),
    };
    Ok((
        content_type(extension),
        bytes.to_vec(),
        Some(state.play.generation),
    ))
}

fn provider_artifact(state: &RwLock<DevState>, query: Option<&str>) -> Served {
    let state = state.read().expect("state lock");
    let Some(good) = &state.play.good else {
        return Err((
            503,
            "no good Play build yet — fix the project diagnostics".to_string(),
        ));
    };
    let Some(module) = &good.provider_js else {
        return Err((404, "the Play entry has no provider module".to_string()));
    };
    let requested_hash = query.and_then(|query| {
        query
            .split('&')
            .find_map(|part| part.strip_prefix("sha256="))
    });
    let actual_hash = sha256_hex(module.as_bytes());
    if requested_hash.is_some_and(|expected| expected != actual_hash) {
        return Err((
            409,
            "the provider changed after config.json was fetched — reload the page".to_string(),
        ));
    }
    Ok((
        content_type("js"),
        module.clone().into_bytes(),
        Some(state.play.generation),
    ))
}

fn app_document(web: &WebAssets) -> Served {
    Ok((content_type("html"), web.index.as_ref().clone(), None))
}

fn application_path(web: &WebAssets, path: &str) -> Served {
    let Some(encoded_relative) = path.strip_prefix('/') else {
        return app_document(web);
    };
    if encoded_relative.is_empty() {
        return app_document(web);
    }
    let relative = decode_asset_path(encoded_relative, "bad application asset path")?;
    if relative == "assets" {
        return Err((404, "no such application asset".to_string()));
    }
    if web.files.contains_key(&relative) {
        return serve_web_file(web, &relative);
    }
    if relative.starts_with("assets/") {
        return serve_web_file(web, &relative);
    }
    if relative == "favicon.ico" {
        return favicon(web);
    }
    app_document(web)
}

fn serve_web_file(web: &WebAssets, relative: &str) -> Served {
    let file = web
        .files
        .get(relative)
        .ok_or_else(|| (404, format!("no such application asset: /{relative}")))?;
    Ok((file.content_type.clone(), file.bytes.as_ref().clone(), None))
}

fn favicon(web: &WebAssets) -> Served {
    match serve_web_file(web, "favicon.ico") {
        Ok(file) => Ok(file),
        Err((404, _)) => Ok((content_type("ico"), Vec::new(), None)),
        Err(error) => Err(error),
    }
}

fn play_asset(state: &RwLock<DevState>, encoded_relative: &str) -> Served {
    let state = state.read().expect("state lock");
    let Some(good) = &state.play.good else {
        return Err((
            503,
            "no good Play build yet — fix the project diagnostics".to_string(),
        ));
    };
    captured_play_asset(&good.play_assets, encoded_relative)
}

fn captured_play_asset(assets: &BTreeMap<String, Arc<[u8]>>, encoded_relative: &str) -> Served {
    let relative = decode_play_asset_path(encoded_relative)?;
    let bytes = assets
        .get(&relative)
        .ok_or_else(|| (404, format!("no such Play asset: {relative}")))?;
    let extension = Path::new(&relative)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    Ok((content_type(extension), bytes.to_vec(), None))
}

/// Decode one URL-path suffix without giving an encoded percent sign a second
/// interpretation. Asset identities may contain spaces and safe nested `/`
/// separators, but the decoded result must remain a lexical relative path.
fn decode_play_asset_path(encoded: &str) -> Result<String, (u16, String)> {
    decode_asset_path(encoded, "bad asset path")
}

fn decode_asset_path(encoded: &str, error_prefix: &str) -> Result<String, (u16, String)> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }

        let Some(high) = bytes.get(index + 1).and_then(|byte| hex_value(*byte)) else {
            return Err((400, format!("{error_prefix}: malformed percent escape")));
        };
        let Some(low) = bytes.get(index + 2).and_then(|byte| hex_value(*byte)) else {
            return Err((400, format!("{error_prefix}: malformed percent escape")));
        };
        decoded.push((high << 4) | low);
        index += 3;
    }

    let decoded = String::from_utf8(decoded)
        .map_err(|_| (400, format!("{error_prefix}: decoded path is not UTF-8")))?;
    let path = Path::new(&decoded);
    if decoded.contains(['\\', '\0'])
        || decoded
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err((400, error_prefix.to_string()));
    }
    Ok(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn serve_file_map(files: &BTreeMap<String, WebFile>, encoded_relative: &str) -> Served {
    let relative = decode_asset_path(encoded_relative, "bad path")?;
    let file = files
        .get(&relative)
        .ok_or_else(|| (404, format!("no such bundled file: {relative}")))?;
    Ok((file.content_type.clone(), file.bytes.as_ref().clone(), None))
}

fn content_type(extension: &str) -> String {
    match extension {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "jpg" | "jpeg" => "image/jpeg",
        "mp4" => "video/mp4",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "png" => "image/png",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let value = u32::from_be_bytes([0, chunk[0], second, third]);
        out.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::io::Read;
    use std::sync::mpsc::sync_channel;
    use std::sync::{Arc, Mutex, Weak};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use uhura_base::{SourceMap, sha256_hex, to_canonical_json};
    use uhura_core::ir::{
        ConstructorDef as MachineConstructorDef, Expr as MachineExpr, Machine as MachineDef,
        PortDef as MachinePortDef, SourceRef,
    };
    use uhura_core::{
        EVIDENCE_REPORT_PROTOCOL, EvidenceArtifacts, EvidenceExampleArtifact, EvidenceFailure,
        EvidenceFailureCode, EvidenceRef, EvidenceReport, EvidenceSnapshot, InstanceLifecycle,
        MACHINE_PROGRAM_ID_PROTOCOL, Program, ScenarioReport, ScenarioStatus, TypeDef, TypeRef,
        Value,
    };
    use uhura_editor_model::{
        Application, AuthoringMetadata, EditorRender, Preview, RenderFreshness,
    };

    use super::{
        APPLICATION_PROVIDER_ADAPTER, ApiRoute, BLOCKING_EVENT_STREAM_WRITE_BOUNDARY,
        ClientCandidate, ClientRegistry, EditorBuildArtifact, EditorBuildRejection,
        EditorHostState, EventStream, EventStreamPoll, EvidenceSummary, GoodBuild, Host,
        IconFontFamilyResource, IconFontResources, MAX_EVENT_STREAMS_PER_HOST,
        PlayAdmissionRejection, PlayArtifact, ProjectSourceFingerprint, RequestMethod, RouteBody,
        RoutePathClaim, RoutePathDecode, RoutePathScope, RouteRequest, RouteResponse,
        UHURA_EVIDENCE_SUMMARY_PROTOCOL, WEB_HISTORY_ADAPTER, WebAssets, WebFile, api_route,
        app_document, application_path, broadcast, captured_play_asset, content_type,
        decode_play_asset_path, deployment_identity, editor_sse_payload, evidence_failure,
        index_asset_references, load_web_app_from, parse_host_manifest, serve_file_map,
        split_request_url, subscribe, subscribe_with_blocking_keepalive, tool_root,
        validate_deployment,
    };

    const TEST_PROVIDER_JS: &str = "export const provider = {};";

    fn render(revision: u64, name: &str) -> EditorRender {
        EditorRender {
            revision,
            freshness: RenderFreshness::Current,
            application: Application {
                name: name.to_string(),
            },
            authoring: AuthoringMetadata::default(),
            groups: Vec::new(),
            previews: Vec::new(),
            stylesheet: String::new(),
            assets: BTreeMap::new(),
            interaction_graph: Default::default(),
            machine: None,
        }
    }

    fn artifact(revision: u64, name: &str) -> EditorBuildArtifact {
        EditorBuildArtifact {
            render: render(revision, name),
            icon_fonts: None,
            preview_count: 0,
            replay_derived_count: 0,
            diagnostics: serde_json::Value::Null,
        }
    }

    fn icon_fonts(bytes: &[u8]) -> IconFontResources {
        let font_hash = sha256_hex(bytes);
        IconFontResources {
            default: "foundation".to_string(),
            families: BTreeMap::from([(
                "foundation".to_string(),
                IconFontFamilyResource {
                    font: Arc::<[u8]>::from(bytes),
                    font_hash,
                    glyphs: BTreeMap::from([
                        ("heart".to_string(), 0xe001),
                        ("home".to_string(), 0xe000),
                    ]),
                },
            )]),
        }
    }

    fn artifact_with_icon_fonts(
        revision: u64,
        name: &str,
        icon_fonts: IconFontResources,
    ) -> EditorBuildArtifact {
        EditorBuildArtifact {
            icon_fonts: Some(icon_fonts),
            ..artifact(revision, name)
        }
    }

    fn uhura_editor_artifact(revision: u64) -> EditorBuildArtifact {
        EditorBuildArtifact {
            render: render(revision, "machine"),
            icon_fonts: None,
            preview_count: 0,
            replay_derived_count: 0,
            diagnostics: serde_json::Value::Null,
        }
    }

    fn good_build(icon_fonts: IconFontResources) -> GoodBuild {
        GoodBuild {
            diagnostics: serde_json::Value::Null,
            ir: "{}".to_string(),
            inspect_json: "{}".to_string(),
            stylesheet: String::new(),
            config_json: "{}".to_string(),
            provider_js: None,
            play_assets: BTreeMap::new(),
            icon_fonts: Some(icon_fonts),
        }
    }

    fn uhura_good_build() -> GoodBuild {
        GoodBuild {
            diagnostics: serde_json::Value::Null,
            ir: "{}".into(),
            inspect_json: "{}".into(),
            stylesheet: String::new(),
            config_json: "{}".into(),
            provider_js: None,
            play_assets: BTreeMap::new(),
            icon_fonts: None,
        }
    }

    fn accepted_editor_render(candidate: &ClientCandidate) -> serde_json::Value {
        match &candidate.editor {
            Ok(artifact) => artifact.render.to_json(),
            Err(rejection) => panic!(
                "expected a current-language Editor artifact, got diagnostics: {:?}",
                rejection.diagnostics
            ),
        }
    }

    fn inspection_graph_source_paths(inspection: &serde_json::Value) -> BTreeSet<String> {
        let inventory = inspection["sources"]
            .as_array()
            .expect("inspection source inventory")
            .iter()
            .map(|source| {
                (
                    source["path"].as_str().expect("source path"),
                    source["bytes"].as_u64().expect("source byte length"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let graph_sources = &inspection["graphSources"];
        let mut paths = BTreeSet::new();
        for entry in graph_sources["nodes"]
            .as_array()
            .expect("graph node sources")
            .iter()
            .chain(
                graph_sources["edges"]
                    .as_array()
                    .expect("graph edge sources"),
            )
        {
            let sources = entry["sources"]
                .as_array()
                .expect("graph provenance source list");
            assert!(!sources.is_empty());
            for source in sources {
                let path = source["path"].as_str().expect("graph source path");
                let start = source["start"].as_u64().expect("graph source start");
                let end = source["end"].as_u64().expect("graph source end");
                let bytes = inventory
                    .get(path)
                    .unwrap_or_else(|| panic!("graph source `{path}` is outside the inventory"));
                assert!(start <= end);
                assert!(end <= *bytes);
                paths.insert(path.to_string());
            }
        }
        paths
    }

    fn evidence_report_with_payload(payload: &str) -> EvidenceReport {
        let source = SourceRef::synthetic("evidence-summary-test");
        let snapshot = EvidenceSnapshot {
            machine: "example@1::Machine".into(),
            machine_program_hash: "a".repeat(64),
            instance: "example-1".into(),
            configuration: Value::Text(payload.into()),
            state: Value::Text(payload.into()),
            observation: Value::Text(payload.into()),
            inbox: vec![Value::Text(payload.into())],
            lifecycle: InstanceLifecycle::Running,
            next_sequence: 1,
            fixtures: BTreeMap::new(),
            trace_prefix_hash: "b".repeat(64),
        };
        let mut artifacts = EvidenceArtifacts::default();
        artifacts.examples.insert(
            "default".into(),
            EvidenceExampleArtifact {
                name: "default".into(),
                reference: EvidenceRef {
                    scenario: "canonical".into(),
                    pin: "frame".into(),
                },
                source: source.clone(),
                metadata: Default::default(),
                observation: Value::Text(payload.into()),
                snapshot: snapshot.clone(),
            },
        );
        EvidenceReport {
            protocol: EVIDENCE_REPORT_PROTOCOL.into(),
            passed: true,
            scenarios: vec![ScenarioReport {
                scenario: "canonical".into(),
                machine: Some("example@1::Machine".into()),
                status: ScenarioStatus::Passed,
                total_steps: 1,
                executed_steps: 1,
                genesis: None,
                receipts: Vec::new(),
                final_snapshot: Some(snapshot),
                published_pins: vec!["frame".into()],
                failure: None,
            }],
            artifacts,
            failures: Vec::new(),
        }
    }

    #[test]
    fn evidence_summary_is_independent_of_runtime_payload_size() {
        let small = EvidenceSummary::from_report(&evidence_report_with_payload("small"));
        let large_payload = "play-evidence-large-sentinel".repeat(64 * 1024);
        let large = EvidenceSummary::from_report(&evidence_report_with_payload(&large_payload));

        assert_eq!(small, large);
        let encoded = to_canonical_json(&large.to_json());
        assert!(!encoded.contains("play-evidence-large-sentinel"));
        assert_eq!(
            large.to_json(),
            serde_json::json!({
                "protocol": "uhura-evidence-summary/0",
                "passed": true,
                "scenarios": { "total": 1, "passed": 1, "failed": 0 },
                "artifacts": { "pins": 0, "examples": 1, "checkpoints": 0 },
                "failureCount": 0,
            }),
        );
    }

    #[test]
    fn evidence_summary_reports_failed_scenarios_and_failures() {
        let mut report = evidence_report_with_payload("failed");
        let failure = EvidenceFailure {
            code: EvidenceFailureCode::ExpectationMismatch,
            scenario: Some("canonical".into()),
            step_index: Some(0),
            source_id: "evidence-summary-test".into(),
            source: SourceRef::synthetic("evidence-summary-test"),
            message: "the authored expectation failed".into(),
        };
        report.passed = false;
        report.scenarios[0].status = ScenarioStatus::Failed;
        report.scenarios[0].failure = Some(failure.clone());
        report.failures.push(failure);

        assert_eq!(
            EvidenceSummary::from_report(&report).to_json(),
            serde_json::json!({
                "protocol": "uhura-evidence-summary/0",
                "passed": false,
                "scenarios": { "total": 1, "passed": 0, "failed": 1 },
                "artifacts": { "pins": 0, "examples": 1, "checkpoints": 0 },
                "failureCount": 1,
            }),
        );
    }

    fn copy_a0_fixture(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("uhura-a0-{label}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = tool_root().join("examples/applications/a0-return-desk/answers/uhura-0.4");
        fs::create_dir_all(root.join("evidence")).unwrap();
        for name in [
            "uhura.toml",
            "machine.uhura",
            "ui.uhura",
            "evidence-support.uhura",
            "host.toml",
            "provider.mjs",
        ] {
            fs::copy(source.join(name), root.join(name)).unwrap();
        }
        fs::copy(
            source.join("evidence/conformance.uhura"),
            root.join("evidence/conformance.uhura"),
        )
        .unwrap();
        root
    }

    const COUNTER_MACHINE_SOURCE: &str = r#"pub machine Counter {
  events {
    Increment,
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = 0,
  }

  observe {
    count,
  }

  on Increment {
    count = count + 1;
    Accepted
  }
}
"#;

    fn fixture_project(label: &str, manifest_extra: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-host-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("uhura.toml"),
            format!(
                r#"[project]
name = "test.counter"
version = 1
language = "0.4"

[modules]
counter = "counter.uhura"

[icons]
default = "lucide"

{manifest_extra}"#
            ),
        )
        .unwrap();
        fs::write(root.join("counter.uhura"), COUNTER_MACHINE_SOURCE).unwrap();
        root
    }

    fn route_fixture_project(label: &str, pattern: &str) -> std::path::PathBuf {
        let root = fixture_project(label, "");
        let location = if pattern.contains("{segment}") {
            "Page { segment: Text }"
        } else {
            "Page"
        };
        fs::write(
            root.join("counter.uhura"),
            format!(
                r#"use uhura::web_router::{{Router, Routes}};

pub enum Location {{
  {location},
}}

pub const ROUTES: Routes<Location> = Routes::from([
  ("Page", "{pattern}"),
]);

pub machine Counter {{
  port router = Router<Location> {{ routes: ROUTES }};

  outcomes {{
    commit Accepted,
  }}

  on router.Changed(location) {{
    Accepted
  }}
}}"#
            ),
        )
        .unwrap();
        fs::write(
            root.join("host.toml"),
            r#"[entry.counter]
machine = "crate::Counter"
lifetime = "application-session"

[entry.counter.ports]
router = "web.history"
"#,
        )
        .unwrap();
        root
    }

    #[test]
    fn standalone_web_route_ownership_rejects_play_but_keeps_editor_current() {
        for (label, pattern) in [
            ("play", "/play"),
            ("editor", "/_uhura/editor"),
            ("api-child", "/api/application"),
            ("assets", "/assets"),
            ("asset-child", "/assets/application.js"),
            ("encoded-asset-child", "/assets%2Fapplication.js"),
            ("favicon", "/favicon.ico"),
            ("dynamic", "/{segment}"),
        ] {
            let root = route_fixture_project(label, pattern);
            let candidate =
                super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
            assert!(
                candidate.summary().editor_current,
                "{pattern} editor diagnostics: {:#}",
                candidate.diagnostics().editor
            );
            assert!(!candidate.summary().play_ok, "{pattern}");
            assert_eq!(
                candidate.checked_route_patterns().unwrap()[0].display_pattern(),
                pattern
            );
            for diagnostics in [candidate.diagnostics().editor, candidate.diagnostics().play] {
                assert_eq!(
                    diagnostics["diagnostics"][0]["rule"], "uhura/reserved-web-host-route",
                    "{pattern}: {diagnostics:#}"
                );
                assert!(
                    diagnostics["diagnostics"][0]["message"]
                        .as_str()
                        .is_some_and(|message| message.contains(pattern)),
                    "{pattern}: {diagnostics:#}"
                );
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn standalone_web_route_admission_is_segment_aware() {
        for (label, pattern) in [
            ("api-root", "/api"),
            ("encoded-api-child", "/api%2Fapplication"),
            ("play-child", "/play/child"),
            ("editor-child", "/_uhura/editor/child"),
        ] {
            let root = route_fixture_project(label, pattern);
            let candidate =
                super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
            assert!(
                candidate.summary().play_ok,
                "{pattern} diagnostics: {:#}",
                candidate.diagnostics().play
            );
            assert_eq!(
                candidate.checked_route_patterns().unwrap()[0].display_pattern(),
                pattern
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn checked_route_claims_preserve_raw_and_decoded_path_semantics() {
        let dynamic_root = route_fixture_project("claim-dynamic", "/{segment}");
        let candidate =
            super::build_candidate(&crate::source::capture_project_snapshot(&dynamic_root), 1);
        let route = &candidate.checked_route_patterns().unwrap()[0];
        assert!(route.overlaps(RoutePathClaim {
            path: "/play",
            scope: RoutePathScope::Exact,
            decode: RoutePathDecode::Raw,
        }));
        assert!(!route.overlaps(RoutePathClaim {
            path: "/api",
            scope: RoutePathScope::Descendants,
            decode: RoutePathDecode::Raw,
        }));
        assert!(route.overlaps(RoutePathClaim {
            path: "/graphql",
            scope: RoutePathScope::Namespace,
            decode: RoutePathDecode::PercentDecodedOnce,
        }));
        assert!(route.overlaps(RoutePathClaim {
            path: "/~",
            scope: RoutePathScope::Prefix,
            decode: RoutePathDecode::PercentDecodedOnce,
        }));
        fs::remove_dir_all(dynamic_root).unwrap();

        let encoded = route_fixture_project("claim-encoded", "/graphql%2Fv2");
        let candidate =
            super::build_candidate(&crate::source::capture_project_snapshot(&encoded), 1);
        let route = &candidate.checked_route_patterns().unwrap()[0];
        assert!(!route.overlaps(RoutePathClaim {
            path: "/graphql",
            scope: RoutePathScope::Namespace,
            decode: RoutePathDecode::Raw,
        }));
        assert!(route.overlaps(RoutePathClaim {
            path: "/graphql",
            scope: RoutePathScope::Namespace,
            decode: RoutePathDecode::PercentDecodedOnce,
        }));
        fs::remove_dir_all(encoded).unwrap();
    }

    #[test]
    fn route_admission_ignores_unselected_route_tables() {
        let root = fixture_project("unselected-route-table", "");
        fs::write(
            root.join("counter.uhura"),
            r#"use uhura::web_router::{Router, Routes};

pub enum Location {
  Page,
}

pub enum UnusedLocation {
  Reserved,
}

pub const ROUTES: Routes<Location> = Routes::from([
  ("Page", "/page"),
]);

pub const UNUSED_ROUTES: Routes<UnusedLocation> = Routes::from([
  ("Reserved", "/assets"),
]);

pub machine Counter {
  port router = Router<Location> { routes: ROUTES };

  outcomes {
    commit Accepted,
  }

  on router.Changed(location) {
    Accepted
  }
}
"#,
        )
        .unwrap();
        fs::write(
            root.join("host.toml"),
            r#"[entry.counter]
machine = "crate::Counter"
lifetime = "application-session"

[entry.counter.ports]
router = "web.history"
"#,
        )
        .unwrap();
        let candidate = super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        assert!(
            candidate.summary().play_ok,
            "unused route table diagnostics: {:#}",
            candidate.diagnostics().play
        );
        let routes = candidate.checked_route_patterns().unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].display_pattern(), "/page");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_exposes_checked_route_patterns_without_source_or_ir_reparsing() {
        let root = copy_a0_fixture("checked-route-view");
        let candidate = super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        assert!(
            candidate.summary().play_ok,
            "play diagnostics: {:#}",
            candidate.diagnostics().play
        );
        let routes = candidate
            .checked_route_patterns()
            .expect("usable Play candidate has a checked route view");
        let view = routes
            .iter()
            .map(|route| (route.table(), route.constructor(), route.display_pattern()))
            .collect::<Vec<_>>();
        assert_eq!(
            view,
            vec![
                (
                    "app.returndesk@1::RETURN_ROUTES",
                    "Flow",
                    "/orders/{order}/return?step={step?}",
                ),
                (
                    "app.returndesk@1::RETURN_ROUTES",
                    "Order",
                    "/orders/{order}",
                ),
                (
                    "app.returndesk@1::RETURN_ROUTES",
                    "Receipt",
                    "/returns/{return_id}",
                ),
            ]
        );

        fs::write(root.join("machine.uhura"), "not valid Uhura").unwrap();
        let rejected = super::build_candidate(&crate::source::capture_project_snapshot(&root), 2);
        assert!(rejected.checked_route_patterns().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn aggregate_play_rejection_keeps_current_editor_and_last_good_play() {
        let root = copy_a0_fixture("aggregate-play-rejection");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let initial = super::build_candidate(&snapshot, 1);
        let (host, initial_report) = Host::new(test_web_assets(), initial).unwrap();
        assert!(initial_report.editor_current);
        assert!(initial_report.play_ok);

        let mut rejected = super::build_candidate(&snapshot, 2);
        rejected.reject_play_admission(PlayAdmissionRejection::new(
            "SPK1001",
            "spock/reserved-client-route",
            "checked route overlaps an aggregate-host namespace",
        ));
        assert!(rejected.summary().editor_current);
        assert!(!rejected.summary().play_ok);
        assert!(!rejected.checked_route_patterns().unwrap().is_empty());

        let report = host.publish(rejected).unwrap();
        assert!(report.editor_current);
        assert!(!report.play_ok);
        assert!(report.has_good_play);
        let editor: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/editor/state",
            })))
            .unwrap();
        assert_eq!(editor["sourceRevision"], 2);
        assert_eq!(
            editor["diagnostics"]["diagnostics"][0]["rule"],
            "spock/reserved-client-route"
        );
        let play = host.route(RouteRequest {
            method: RequestMethod::Get,
            url: "/api/play/ir.json",
        });
        assert_eq!(play.status, 200, "last-good Play remains served");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mixed_presentation_taxonomy_has_one_deterministic_authoring_owner() {
        use uhura_editor_model::PreviewKind;

        let mut forward = BTreeMap::new();
        for kind in [
            PreviewKind::Component,
            PreviewKind::Surface,
            PreviewKind::Page,
        ] {
            super::retain_presentation_owner_kind(&mut forward, "test::Shared", kind);
        }

        let mut reverse = BTreeMap::new();
        for kind in [
            PreviewKind::Page,
            PreviewKind::Surface,
            PreviewKind::Component,
        ] {
            super::retain_presentation_owner_kind(&mut reverse, "test::Shared", kind);
        }

        assert_eq!(forward, reverse);
        assert_eq!(forward["test::Shared"], PreviewKind::Page);
    }

    #[test]
    fn composed_replay_dispatch_keeps_aggregate_runtime_identity() {
        use uhura_core::{
            OutcomePolicy, Provenance, ProvenanceOccurrence, ProvenanceSource,
            REACTION_RECEIPT_PROTOCOL, ReactionReceipt, ReactionResolution, Value,
            semantic_node_id,
        };

        let root = fixture_project("composed-replay", "");
        let checked =
            super::check_project_snapshot(&crate::source::capture_project_snapshot(&root))
                .expect("checked 0.4 project");
        let mut program = checked.program;
        let machine_id = program
            .machine_program
            .machines
            .keys()
            .next()
            .expect("counter machine")
            .clone();
        let machine = program
            .machine_program
            .machines
            .get_mut(&machine_id)
            .unwrap();
        let mut handler = machine.handlers.remove("Increment").unwrap();
        handler.input = "controls.Increment".into();
        machine
            .handlers
            .insert("controls.Increment".into(), handler);
        let input_type = machine.local_input.id().to_string();

        let source = COUNTER_MACHINE_SOURCE;
        let provenance = Provenance::canonical(
            vec![ProvenanceSource {
                source: 0,
                package: "test.counter@1".into(),
                module: "counter".into(),
                path: "counter.uhura".into(),
                sha256: sha256_hex(source.as_bytes()),
                bytes: source.len() as u64,
            }],
            vec![ProvenanceOccurrence {
                node: semantic_node_id(&machine_id, "controls", "handler", "handler/Increment/0"),
                source: 0,
                start: 0,
                end: 1,
                role: "definition".into(),
                owner: "controls".into(),
            }],
        )
        .unwrap();
        let receipt = ReactionReceipt {
            protocol: REACTION_RECEIPT_PROTOCOL.into(),
            instance: "entry/counter".into(),
            machine_program_hash: "program".into(),
            configuration_hash: "configuration".into(),
            sequence: 1,
            input: Value::Variant {
                type_id: input_type,
                constructor: "controls.Increment".into(),
                fields: Vec::new(),
            },
            resolution: ReactionResolution::Completed {
                outcome: Value::Unit,
                policy: OutcomePolicy::Commit,
            },
            ordered_commands: Vec::new(),
            post_observation: Value::Unit,
            pre_state_hash: "before".into(),
            post_state_hash: "after".into(),
        };

        assert_eq!(
            super::replay_handler_owner(&machine_id, "controls.Increment", &provenance,),
            "controls"
        );
        let dispatch = super::replay_dispatch(&program, &machine_id, &provenance, &receipt);
        assert_eq!(dispatch["scope"], "entry/counter");
        assert_eq!(dispatch["definition"], machine_id);
        assert_eq!(dispatch["on"], "controls.Increment");
        assert!(!dispatch["scope"].as_str().unwrap().contains("/part/"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn host_checks_manifest_selected_project_with_shared_resources() {
        let root = fixture_project("accepted", "");
        let snapshot = crate::source::capture_project_snapshot(&root);

        let checked = super::check_project_snapshot(&snapshot).expect("checked 0.4 project");
        assert_eq!(checked.program.machine_program.language, "uhura 0.4");
        assert_eq!(checked.program.machine_program.machines.len(), 1);
        assert_eq!(checked.sources.len(), 1);
        assert_eq!(checked.semantic_provenance.protocol, "uhura-provenance/0",);
        assert_eq!(checked.semantic_provenance.sources[0].module, "counter",);
        assert_eq!(checked.semantic_provenance.sources[0].path, "counter.uhura",);
        assert!(!checked.semantic_provenance.occurrences.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn successful_checked_projects_require_semantic_provenance() {
        let diagnostics = super::require_semantic_provenance(None).unwrap_err();
        assert_eq!(diagnostics["diagnostics"][0]["code"], "R3014");
        assert_eq!(diagnostics["diagnostics"][0]["rule"], "uhura/provenance");
        assert!(
            diagnostics["diagnostics"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("produced no semantic provenance"))
        );
    }

    #[test]
    fn host_requires_the_current_project_manifest() {
        for (label, manifest) in [
            ("missing-manifest", None),
            (
                "resource-only-manifest",
                Some(
                    r#"[icons]
default = "lucide"
"#,
                ),
            ),
        ] {
            let root = fixture_project(label, "");
            if let Some(manifest) = manifest {
                fs::write(root.join("uhura.toml"), manifest).unwrap();
            } else {
                fs::remove_file(root.join("uhura.toml")).unwrap();
            }

            let rejected =
                super::check_project_snapshot(&crate::source::capture_project_snapshot(&root))
                    .err()
                    .expect("implicit and resource-only projects must be rejected");
            assert_eq!(rejected["diagnostics"][0]["code"], "UH2001");
            assert!(
                rejected.to_string().contains("project")
                    && rejected.to_string().contains("modules")
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn host_rejects_unknown_icon_tokens_before_editor_or_play_publication() {
        let root = fixture_project("unknown-icon", "");
        fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.counter"
version = 1
language = "0.4"

[modules]
counter = "counter.uhura"
ui = "ui.uhura"

[icons]
default = "lucide"
"#,
        )
        .unwrap();
        fs::write(
            root.join("ui.uhura"),
            r#"use uhura::ui;
use crate::counter::Counter;

pub ui CounterWeb for Counter(view) {
  <button label="Increment" on press -> Increment>
    <icon name="definitely-not-a-lucide-glyph" />
  </button>
}
"#,
        )
        .unwrap();

        let rejected =
            super::check_project_snapshot(&crate::source::capture_project_snapshot(&root))
                .err()
                .expect("unknown icon must gate host publication");
        let diagnostic = rejected["diagnostics"]
            .as_array()
            .and_then(|diagnostics| {
                diagnostics
                    .iter()
                    .find(|diagnostic| diagnostic["rule"] == "uhura/unknown-icon")
            })
            .expect("unknown icon diagnostic");
        assert_eq!(diagnostic["code"], "UH5017");
        assert_eq!(diagnostic["file"], "ui.uhura");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn host_rejects_unlisted_sources_and_admits_locked_path_dependencies() {
        let unlisted_root = fixture_project("unlisted", "");
        fs::write(unlisted_root.join("stray.uhura"), COUNTER_MACHINE_SOURCE).unwrap();
        let unlisted =
            super::check_project_snapshot(&crate::source::capture_project_snapshot(&unlisted_root))
                .err()
                .expect("unlisted source must be rejected");
        assert!(
            unlisted.to_string().contains("stray.uhura")
                && unlisted.to_string().contains("not listed")
        );
        fs::remove_dir_all(unlisted_root).unwrap();

        let dependency_root = fixture_project(
            "dependency",
            r#"[dependencies.shared]
package = "test.shared"
version = 1
path = "vendor/shared"
"#,
        );
        fs::create_dir_all(dependency_root.join("vendor/shared")).unwrap();
        fs::write(
            dependency_root.join("counter.uhura"),
            COUNTER_MACHINE_SOURCE
                .replace("pub machine", "use shared::values::INITIAL;\n\npub machine")
                .replace("count: Int = 0", "count: Int = INITIAL"),
        )
        .unwrap();
        let vendor_manifest_text = r#"[project]
name = "test.shared"
version = 1
language = "0.4"

[modules]
values = "values.uhura"
"#;
        let vendor_source = b"pub const INITIAL: Int = 0;\n";
        fs::write(
            dependency_root.join("vendor/shared/uhura.toml"),
            vendor_manifest_text,
        )
        .unwrap();
        fs::write(
            dependency_root.join("vendor/shared/values.uhura"),
            vendor_source,
        )
        .unwrap();
        let vendor_manifest =
            uhura_check::project_manifest::load_project_manifest(vendor_manifest_text).unwrap();
        let vendor_capture = uhura_check::project_lock::CapturedPackage {
            manifest: vendor_manifest,
            source: uhura_check::project_manifest::ProjectPath::parse("vendor/shared").unwrap(),
            modules: [(
                uhura_check::project_manifest::LogicalModulePath::parse("values").unwrap(),
                vendor_source.to_vec(),
            )]
            .into_iter()
            .collect(),
            resolved_dependencies: BTreeMap::new(),
            resources: BTreeMap::new(),
        };
        let integrity = vendor_capture.artifact_integrity().unwrap();
        fs::write(
            dependency_root.join("uhura.lock"),
            format!(
                r#"protocol = "uhura-lock/0"

[root]
package = "test.counter@1"
dependencies = {{ shared = "test.shared@1" }}

[[package]]
package = "test.shared@1"
source = {{ kind = "path", path = "vendor/shared" }}
integrity = "{integrity}"
dependencies = {{}}
"#
            ),
        )
        .unwrap();
        let dependency = super::check_project_snapshot(&crate::source::capture_project_snapshot(
            &dependency_root,
        ))
        .expect("locked dependency checks");
        assert!(
            dependency
                .program
                .machine_program
                .machines
                .contains_key("test.counter@1::Counter")
        );
        assert_eq!(dependency.selector_packages["shared"], "test.shared@1");

        let invalid_vendor_source = b"pub const INITIAL: Int = ;\n";
        let expected_parse = uhura_syntax::parse(
            uhura_syntax::SourceIdentity::new(
                0,
                "test.shared@1",
                "values",
                "vendor/shared/values.uhura",
            ),
            std::str::from_utf8(invalid_vendor_source).unwrap(),
        )
        .diagnostics
        .into_iter()
        .next()
        .expect("invalid dependency has a parser diagnostic");
        let (expected_code, expected_rule) = expected_parse.kind.diagnostic_identity();
        fs::write(
            dependency_root.join("vendor/shared/values.uhura"),
            invalid_vendor_source,
        )
        .unwrap();
        let invalid_vendor_manifest =
            uhura_check::project_manifest::load_project_manifest(vendor_manifest_text).unwrap();
        let invalid_vendor_capture = uhura_check::project_lock::CapturedPackage {
            manifest: invalid_vendor_manifest,
            source: uhura_check::project_manifest::ProjectPath::parse("vendor/shared").unwrap(),
            modules: [(
                uhura_check::project_manifest::LogicalModulePath::parse("values").unwrap(),
                invalid_vendor_source.to_vec(),
            )]
            .into_iter()
            .collect(),
            resolved_dependencies: BTreeMap::new(),
            resources: BTreeMap::new(),
        };
        let invalid_integrity = invalid_vendor_capture.artifact_integrity().unwrap();
        fs::write(
            dependency_root.join("uhura.lock"),
            format!(
                r#"protocol = "uhura-lock/0"

[root]
package = "test.counter@1"
dependencies = {{ shared = "test.shared@1" }}

[[package]]
package = "test.shared@1"
source = {{ kind = "path", path = "vendor/shared" }}
integrity = "{invalid_integrity}"
dependencies = {{}}
"#
            ),
        )
        .unwrap();
        let diagnostics = super::check_project_snapshot(&crate::source::capture_project_snapshot(
            &dependency_root,
        ))
        .err()
        .expect("dependency parse failure must reject the project");
        let parse = diagnostics["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|diagnostic| diagnostic["rule"] == expected_rule)
            .expect("dependency parser diagnostic");
        assert_eq!(parse["code"], expected_code);
        assert_eq!(parse["file"], "vendor/shared/values.uhura");
        assert!(
            parse["message"]
                .as_str()
                .is_some_and(|message| message.contains("package.test.shared@1.modules.values"))
        );
        assert_eq!(parse["span"]["offset"], expected_parse.span.start);
        assert_eq!(
            parse["span"]["len"],
            expected_parse.span.end - expected_parse.span.start
        );
        assert!(parse["span"]["start"]["line"].as_u64().unwrap() > 0);
        assert!(parse["span"]["start"]["col"].as_u64().unwrap() > 0);
        assert!(parse["span"]["end"]["line"].as_u64().unwrap() > 0);
        assert!(parse["span"]["end"]["col"].as_u64().unwrap() > 0);
        fs::remove_dir_all(dependency_root).unwrap();
    }

    #[test]
    fn split_file_projection_provenance_keeps_physical_owner_identity() {
        use uhura_core::{
            PROJECTION_SOURCES_PROTOCOL, Presentation, ProjectionSources, Provenance,
            ProvenanceOccurrence, ProvenanceSource, UiNode, semantic_node_id,
        };
        use uhura_editor_model::{PreviewKind, SourceTargetOwnerKind};

        let package = "test.widgets@1";
        let card = format!("{package}::Card");
        let screen = format!("{package}::Screen");
        let machine = format!("{package}::Machine");
        let card_source_id = semantic_node_id(&card, "root", "ui", "declaration/Card");
        let screen_source_id = semantic_node_id(&screen, "root", "ui", "declaration/Screen");
        let card_node_id = semantic_node_id(&card, "root", "ui_element", "tree/0/element/view");
        let screen_node_id = semantic_node_id(&screen, "root", "ui_element", "tree/0/element/view");
        let card_text = "<view>Card</view>\n";
        let screen_text = "<view>Screen</view>\n";

        let mut program = Program::new();
        program.presentations.insert(
            card.clone(),
            Presentation {
                id: card.clone(),
                machine: machine.clone(),
                binding: "view".into(),
                nodes: vec![UiNode::Element {
                    name: "view".into(),
                    attributes: Vec::new(),
                    children: Vec::new(),
                    source: SourceRef {
                        id: card_node_id.clone(),
                        path: "<resolved-project>".into(),
                        start: 0,
                        end: card_text.len() as u32,
                    },
                }],
                source: SourceRef {
                    id: card_source_id.clone(),
                    path: "<resolved-project>".into(),
                    start: 0,
                    end: card_text.len() as u32,
                },
            },
        );
        program.presentations.insert(
            screen.clone(),
            Presentation {
                id: screen.clone(),
                machine: machine.clone(),
                binding: "view".into(),
                nodes: vec![UiNode::Element {
                    name: "view".into(),
                    attributes: Vec::new(),
                    children: Vec::new(),
                    source: SourceRef {
                        id: screen_node_id.clone(),
                        path: "<resolved-project>".into(),
                        start: 0,
                        end: screen_text.len() as u32,
                    },
                }],
                source: SourceRef {
                    id: screen_source_id.clone(),
                    path: "<resolved-project>".into(),
                    start: 0,
                    end: screen_text.len() as u32,
                },
            },
        );
        let provenance = Provenance::canonical(
            vec![
                ProvenanceSource {
                    source: 0,
                    package: package.into(),
                    module: "card".into(),
                    path: "components/card.uhura".into(),
                    sha256: sha256_hex(card_text.as_bytes()),
                    bytes: card_text.len() as u64,
                },
                ProvenanceSource {
                    source: 1,
                    package: package.into(),
                    module: "screen".into(),
                    path: "pages/screen.uhura".into(),
                    sha256: sha256_hex(screen_text.as_bytes()),
                    bytes: screen_text.len() as u64,
                },
            ],
            vec![
                ProvenanceOccurrence {
                    node: card_source_id.clone(),
                    source: 0,
                    start: 0,
                    end: card_text.len() as u32,
                    role: "definition".into(),
                    owner: "root".into(),
                },
                ProvenanceOccurrence {
                    node: screen_source_id.clone(),
                    source: 1,
                    start: 0,
                    end: screen_text.len() as u32,
                    role: "definition".into(),
                    owner: "root".into(),
                },
                ProvenanceOccurrence {
                    node: card_node_id.clone(),
                    source: 0,
                    start: 0,
                    end: card_text.len() as u32,
                    role: "definition".into(),
                    owner: "root".into(),
                },
                ProvenanceOccurrence {
                    node: screen_node_id.clone(),
                    source: 1,
                    start: 0,
                    end: screen_text.len() as u32,
                    role: "definition".into(),
                    owner: "root".into(),
                },
                ProvenanceOccurrence {
                    node: semantic_node_id(
                        &machine,
                        "notice_controls",
                        "handler",
                        "handler/DismissNotice/0",
                    ),
                    source: 0,
                    start: 0,
                    end: card_text.len() as u32,
                    role: "generated".into(),
                    owner: "notice_controls".into(),
                },
            ],
        )
        .unwrap();
        let kinds = BTreeMap::from([
            (card.clone(), PreviewKind::Component),
            (screen.clone(), PreviewKind::Page),
        ]);
        let presentation_sources =
            super::presentation_authoring_sources(&program, &kinds, &provenance).unwrap();
        let presentation_node_owners = super::presentation_node_owners(&program);

        let mut source_map = SourceMap::new();
        let card_file = source_map.add("components/card.uhura", card_text);
        let screen_file = source_map.add("pages/screen.uhura", screen_text);
        let source_files = BTreeMap::from([
            ("components/card.uhura".to_string(), card_file),
            ("pages/screen.uhura".to_string(), screen_file),
        ]);
        let projection_authoring = super::ProjectionAuthoringContext {
            source_map: &source_map,
            source_files: &source_files,
            presentation_sources: &presentation_sources,
            presentation_node_owners: &presentation_node_owners,
            provenance: &provenance,
            checked_targets: &[],
        };
        let projection = ProjectionSources {
            protocol: PROJECTION_SOURCES_PROTOCOL.into(),
            presentation: screen.clone(),
            nodes: BTreeMap::from([
                (
                    "card-node".into(),
                    SourceRef {
                        id: card_node_id.clone(),
                        path: "<resolved-project>".into(),
                        start: 0,
                        end: card_text.len() as u32,
                    },
                ),
                (
                    "screen-node".into(),
                    SourceRef {
                        id: screen_node_id.clone(),
                        path: "<resolved-project>".into(),
                        start: 0,
                        end: screen_text.len() as u32,
                    },
                ),
            ]),
        };
        let mut targets = BTreeMap::new();
        let preview = super::projection_provenance(
            "preview",
            &screen,
            &projection,
            &projection_authoring,
            &mut targets,
        )
        .unwrap();

        assert_eq!(preview.occurrences.len(), 2);
        let card_target = &targets[&card_node_id];
        assert_eq!(card_target.file, "components/card.uhura");
        assert_eq!(card_target.owner.kind, SourceTargetOwnerKind::Component);
        assert_eq!(card_target.owner.name, card);
        let screen_target = &targets[&screen_node_id];
        assert_eq!(screen_target.file, "pages/screen.uhura");
        assert_eq!(screen_target.owner.kind, SourceTargetOwnerKind::Page);
        assert_eq!(screen_target.owner.name, screen);
        assert_eq!(
            super::replay_handler_owner(&machine, "notice_controls.DismissNotice", &provenance,),
            "notice_controls"
        );
    }

    fn uhura_program_with_a0_ports() -> Program {
        uhura_program_with_adapter_domain(
            "app.return_desk.machine@1",
            "app.return_desk.machine@1::ReturnDesk",
        )
    }

    fn uhura_program_with_adapter_domain(domain_module: &str, machine_id: &str) -> Program {
        use uhura_port::{
            RouteConstructorDecl, RoutePatternDecl, RouteTable, TypeRef as PortTypeRef,
            observation_instance, request_port_instance, router_instance,
        };

        let named = |name: &str| TypeRef::Named {
            id: format!("{domain_module}::{name}"),
        };
        let routes_id = format!("{domain_module}::test_routes");
        let order_id = named("OrderId");
        let line_id = named("LineId");
        let order_line_wire = named("OrderLineWire");
        let location = named("Location");
        let order_wire = named("OrderWire");
        let request_id = named("RequestId");
        let return_payload = named("ReturnPayload");
        let settlement = named("Settlement");
        let routes = RouteTable::compile(
            PortTypeRef::new(location.canonical_name()).unwrap(),
            vec![RouteConstructorDecl::new("home", Vec::new())],
            vec![RoutePatternDecl::new("home", "/")],
        )
        .unwrap();
        let returns_instance = request_port_instance(
            PortTypeRef::new(request_id.canonical_name()).unwrap(),
            PortTypeRef::new(return_payload.canonical_name()).unwrap(),
            PortTypeRef::new(settlement.canonical_name()).unwrap(),
        )
        .unwrap();
        let router_instance = router_instance(
            PortTypeRef::new(location.canonical_name()).unwrap(),
            &routes,
        )
        .unwrap();
        let orders_instance =
            observation_instance(PortTypeRef::new(order_wire.canonical_name()).unwrap()).unwrap();
        let source = SourceRef::synthetic("test");
        let mut program = Program::new();
        program.machine_program.language = "uhura 0.4".into();
        program.machine_program.identity_protocol = MACHINE_PROGRAM_ID_PROTOCOL.into();
        program.machine_program.types.insert(
            order_id.canonical_name(),
            TypeDef::Key {
                id: order_id.canonical_name(),
                underlying: TypeRef::Text,
            },
        );
        program.machine_program.types.insert(
            line_id.canonical_name(),
            TypeDef::Key {
                id: line_id.canonical_name(),
                underlying: TypeRef::Text,
            },
        );
        program.machine_program.types.insert(
            order_line_wire.canonical_name(),
            TypeDef::Record {
                id: order_line_wire.canonical_name(),
                fields: vec![
                    ("id".into(), line_id),
                    ("title".into(), TypeRef::Text),
                    ("purchased_quantity".into(), TypeRef::Int),
                    ("returnable_quantity".into(), TypeRef::Int),
                    ("policy_summary".into(), TypeRef::Text),
                ],
            },
        );
        program.machine_program.types.insert(
            order_wire.canonical_name(),
            TypeDef::Record {
                id: order_wire.canonical_name(),
                fields: vec![
                    ("id".into(), order_id),
                    ("revision".into(), TypeRef::Int),
                    (
                        "lines".into(),
                        TypeRef::Seq {
                            value: Box::new(order_line_wire),
                        },
                    ),
                    (
                        "allowed_methods".into(),
                        TypeRef::Seq {
                            value: Box::new(TypeRef::Text),
                        },
                    ),
                ],
            },
        );
        program.machine_program.types.insert(
            settlement.canonical_name(),
            TypeDef::Sum {
                id: settlement.canonical_name(),
                constructors: vec![MachineConstructorDef {
                    name: "accepted".into(),
                    fields: vec![(Some("return_id".into()), TypeRef::Text)],
                }],
            },
        );
        program.machine_program.machines.insert(
            machine_id.into(),
            MachineDef {
                id: machine_id.into(),
                config: TypeRef::Unit,
                requires: Vec::new(),
                ports: vec![
                    MachinePortDef {
                        name: "returns".into(),
                        contract: returns_instance.identity.to_string(),
                        contract_instance: Some(returns_instance.clone()),
                        type_arguments: vec![
                            request_id.clone(),
                            return_payload.clone(),
                            settlement.clone(),
                        ],
                        configuration: None,
                        receive: vec![MachineConstructorDef {
                            name: "settled".into(),
                            fields: vec![
                                (Some("id".into()), request_id),
                                (Some("result".into()), settlement),
                            ],
                        }],
                        send: vec![MachineConstructorDef {
                            name: "request".into(),
                            fields: vec![
                                (Some("id".into()), named("RequestId")),
                                (Some("payload".into()), return_payload),
                            ],
                        }],
                        contract_hash: returns_instance.content_hash,
                        source: source.clone(),
                    },
                    MachinePortDef {
                        name: "router".into(),
                        contract: router_instance.identity.to_string(),
                        contract_instance: Some(router_instance.clone()),
                        type_arguments: vec![location.clone()],
                        configuration: Some(MachineExpr::Name {
                            name: routes_id.clone(),
                        }),
                        receive: vec![MachineConstructorDef {
                            name: "changed".into(),
                            fields: vec![(Some("location".into()), location.clone())],
                        }],
                        send: vec![
                            MachineConstructorDef {
                                name: "push".into(),
                                fields: vec![(Some("location".into()), location.clone())],
                            },
                            MachineConstructorDef {
                                name: "replace".into(),
                                fields: vec![(Some("location".into()), location)],
                            },
                            MachineConstructorDef {
                                name: "back".into(),
                                fields: Vec::new(),
                            },
                        ],
                        contract_hash: router_instance.content_hash,
                        source: source.clone(),
                    },
                    MachinePortDef {
                        name: "orders".into(),
                        contract: orders_instance.identity.to_string(),
                        contract_instance: Some(orders_instance.clone()),
                        type_arguments: vec![order_wire.clone()],
                        configuration: None,
                        receive: vec![MachineConstructorDef {
                            name: "observed".into(),
                            fields: vec![(Some("value".into()), order_wire)],
                        }],
                        send: Vec::new(),
                        contract_hash: orders_instance.content_hash,
                        source: source.clone(),
                    },
                ],
                local_input: TypeDef::Sum {
                    id: format!("{machine_id}.Input"),
                    constructors: Vec::new(),
                },
                local_commands: Vec::new(),
                outcomes: Vec::new(),
                state: Vec::new(),
                functions: BTreeMap::new(),
                derives: Vec::new(),
                invariants: Vec::new(),
                observation: Vec::new(),
                transitions: BTreeMap::new(),
                handlers: BTreeMap::new(),
                before_commit: Vec::new(),
                source,
            },
        );
        program.route_tables.insert(routes_id, routes);
        program.freeze_program_hashes();
        program
    }

    fn candidate_with_icon_fonts(revision: u64, bytes: &[u8]) -> ClientCandidate {
        let resources = icon_fonts(bytes);
        ClientCandidate {
            revision,
            source_fingerprint: ProjectSourceFingerprint::default(),
            source_revision_id: "test-source-revision".into(),
            editor: Ok(artifact_with_icon_fonts(
                revision,
                "icons",
                resources.clone(),
            )),
            play: Ok(good_build(resources)),
            checked_routes: Some(Vec::new()),
        }
    }

    fn host_with_icon_fonts(bytes: &[u8]) -> Host {
        Host::new(test_web_assets(), candidate_with_icon_fonts(1, bytes))
            .unwrap()
            .0
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

    fn editor_rejection(message: &str) -> EditorBuildRejection {
        EditorBuildRejection::new(diagnostics(message))
    }

    fn state_json(state: &EditorHostState) -> serde_json::Value {
        serde_json::from_str(&state.state_json).expect("state JSON")
    }

    fn test_host(web: WebAssets) -> Host {
        let candidate = ClientCandidate {
            revision: 1,
            source_fingerprint: ProjectSourceFingerprint::default(),
            source_revision_id: "test-source-revision".into(),
            editor: Ok(artifact(1, "test")),
            play: Err(diagnostics("no Play build")),
            checked_routes: Some(Vec::new()),
        };
        Host::new(web, candidate).unwrap().0
    }

    fn response_bytes(mut response: RouteResponse) -> Vec<u8> {
        let mut bytes = Vec::new();
        response.body.read_to_end(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn editor_transitions_current_to_stale_and_recovers() {
        let mut state = EditorHostState::initial(Ok(artifact(1, "first"))).unwrap();
        let first = state_json(&state);
        assert_eq!(first["sourceRevision"], 1);
        assert_eq!(first["render"]["freshness"], "current");
        assert_eq!(first["render"]["revision"], 1);

        state.apply(2, Err(editor_rejection("broken"))).unwrap();
        let stale = state_json(&state);
        assert_eq!(stale["sourceRevision"], 2);
        assert_eq!(stale["render"]["freshness"], "stale");
        assert_eq!(stale["render"]["revision"], 1);
        assert_eq!(stale["diagnostics"]["diagnostics"][0]["message"], "broken");

        state.apply(3, Ok(artifact(3, "recovered"))).unwrap();
        let recovered = state_json(&state);
        assert_eq!(recovered["sourceRevision"], 3);
        assert_eq!(recovered["render"]["freshness"], "current");
        assert_eq!(recovered["render"]["revision"], 3);
        assert_eq!(recovered["render"]["application"]["name"], "recovered");
        assert_eq!(recovered["diagnostics"], serde_json::Value::Null);
    }

    #[test]
    fn editor_cold_invalid_recovers_without_a_process_restart() {
        let mut state = EditorHostState::initial(Err(editor_rejection("cold"))).unwrap();
        let cold = state_json(&state);
        assert_eq!(cold["sourceRevision"], 1);
        assert_eq!(cold["render"], serde_json::Value::Null);

        state.apply(2, Ok(artifact(2, "ready"))).unwrap();
        let ready = state_json(&state);
        assert_eq!(ready["render"]["freshness"], "current");
        assert_eq!(ready["render"]["revision"], 2);
    }

    #[test]
    fn editor_revisions_are_strictly_monotonic_and_atomic() {
        let mut state = EditorHostState::initial(Ok(artifact(1, "one"))).unwrap();
        let before = state.state_json.clone();
        assert!(state.apply(1, Ok(artifact(1, "old"))).is_err());
        assert!(state.apply(3, Ok(artifact(3, "future"))).is_err());
        assert_eq!(state.source_revision, 1);
        assert_eq!(state.state_json, before);

        state.apply(2, Ok(artifact(2, "two"))).unwrap();
        assert_eq!(state.source_revision, 2);
    }

    #[test]
    fn uhura_editor_last_good_becomes_stale_without_changing_its_render_revision() {
        let cold = EditorHostState::initial(Err(editor_rejection("cold Uhura machine"))).unwrap();
        let cold = state_json(&cold);
        assert_eq!(cold["protocol"], "uhura-editor-state/5");
        assert_eq!(cold["render"], serde_json::Value::Null);

        let mut state = EditorHostState::initial(Ok(uhura_editor_artifact(1))).unwrap();
        let current = state_json(&state);
        assert_eq!(current["protocol"], "uhura-editor-state/5");
        assert_eq!(current["sourceRevision"], 1);
        assert_eq!(current["render"]["revision"], 1);
        assert_eq!(current["render"]["freshness"], "current");

        state
            .apply(2, Err(editor_rejection("broken Uhura machine")))
            .unwrap();
        let stale = state_json(&state);
        assert_eq!(stale["protocol"], "uhura-editor-state/5");
        assert_eq!(stale["sourceRevision"], 2);
        assert_eq!(stale["render"]["revision"], 1);
        assert_eq!(stale["render"]["freshness"], "stale");
        assert_eq!(
            stale["diagnostics"]["diagnostics"][0]["message"],
            "broken Uhura machine"
        );
    }

    #[test]
    fn uhura_host_manifest_is_closed_and_admits_the_sealed_adapter_table() {
        let source = r#"
[entry.return-desk]
machine = "app.return_desk.machine@1::ReturnDesk"
lifetime = "application-session"

[entry.return-desk.ports]
router = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"
"#;
        let deployment = parse_host_manifest(source).unwrap();
        let admitted = validate_deployment(&uhura_program_with_a0_ports(), &deployment)
            .expect("sealed adapter table admits exact compatible bindings")
            .ports;
        assert_eq!(
            admitted
                .iter()
                .map(|binding| binding["port"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["orders", "returns", "router"]
        );
        assert!(admitted.iter().all(|binding| {
            binding["contractInstanceHash"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64)
        }));
        assert!(admitted.iter().all(|binding| {
            binding.as_object().is_some_and(|fields| {
                fields.keys().map(String::as_str).collect::<BTreeSet<_>>()
                    == BTreeSet::from(["adapter", "contractHash", "contractInstanceHash", "port"])
            })
        }));
        assert_eq!(admitted[0]["adapter"], APPLICATION_PROVIDER_ADAPTER);
        assert_eq!(admitted[1]["adapter"], APPLICATION_PROVIDER_ADAPTER);
        assert_eq!(admitted[2]["adapter"], WEB_HISTORY_ADAPTER);

        let unknown = source.replace(
            "lifetime = \"application-session\"",
            "lifetime = \"application-session\"\nambient = true",
        );
        assert!(
            parse_host_manifest(&unknown)
                .unwrap_err()
                .contains("is not allowed")
        );

        let mut incompatible = uhura_program_with_a0_ports();
        incompatible
            .machine_program
            .machines
            .get_mut("app.return_desk.machine@1::ReturnDesk")
            .unwrap()
            .ports[0]
            .contract_hash = "0".repeat(64);
        let (code, message) = validate_deployment(&incompatible, &deployment).unwrap_err();
        assert_eq!(code, "R3015");
        assert!(message.contains("returns"));
    }

    #[test]
    fn host_selectors_resolve_public_ids_and_dotted_part_ports() {
        let source = r#"
[entry.app]
machine = "crate::Application"
presentation = "crate::ApplicationWeb"
lifetime = "application-session"

[entry.app.ports]
"feed.api" = "app.provider"
"#;
        let deployment = parse_host_manifest(source).unwrap();
        assert_eq!(
            deployment
                .ports
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["feed.api"]
        );

        let packages = BTreeMap::from([("crate".to_string(), "example.app@1".to_string())]);
        assert_eq!(
            super::resolve_host_selector(
                "crate::Application",
                "machine",
                &packages,
                ["example.app@1::Application"],
            )
            .unwrap(),
            "example.app@1::Application"
        );
        assert!(
            super::resolve_host_selector(
                "crate::machine::Application",
                "machine",
                &packages,
                ["example.app@1::Application"],
            )
            .unwrap_err()
            .contains("logical-module-qualified")
        );
        assert!(
            super::resolve_host_selector(
                "vendor::Application",
                "machine",
                &packages,
                ["vendor.app@1::Application"],
            )
            .unwrap_err()
            .contains("unknown package alias")
        );
    }

    #[test]
    fn uhura_host_manifest_rejects_recursive_provider_config_floats_before_hashing() {
        let source = r#"
[entry.app]
machine = "test.app@1::App"
lifetime = "application-session"

[entry.app.ports]
data = "app.provider"

[entry.app.provider]
module = "provider.mjs"

[entry.app.provider.config]
attempts = 3
nested = [{ enabled = true }, { ratio = 0.5 }]
"#;
        let error = parse_host_manifest(source).unwrap_err();
        assert_eq!(
            error,
            "host.toml.entry.app.provider.config contains noncanonical deterministic data: $.nested[1].ratio: floating-point JSON number `0.5` is not canonical Uhura data",
        );

        let integer_only = source.replace("ratio = 0.5", "ratio = 5");
        let deployment = parse_host_manifest(&integer_only).unwrap();
        assert_eq!(
            deployment.provider.unwrap().config,
            serde_json::json!({
                "attempts": 3,
                "nested": [{ "enabled": true }, { "ratio": 5 }],
            }),
        );
    }

    #[test]
    fn uhura_host_manifest_admits_exact_typed_configuration_and_hashes_its_value() {
        const MACHINE: &str = "app.return_desk.machine@1::ReturnDesk";
        const LARGE: &str = "900719925474099312345678901234567890";
        let mut program = uhura_program_with_a0_ports();
        program
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .config = TypeRef::Record {
            fields: vec![("seed".into(), TypeRef::Int)],
        };
        program.freeze_program_hashes();

        let source = format!(
            r#"
[entry.return-desk]
machine = "{MACHINE}"
lifetime = "application-session"
configuration = '{{"$":"record","fields":[{{"name":"seed","value":{{"$":"Int","value":"{LARGE}"}}}}]}}'

[entry.return-desk.ports]
router = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"
"#
        );
        let deployment = parse_host_manifest(&source).unwrap();
        let admission = validate_deployment(&program, &deployment)
            .expect("canonical exact configuration is admitted against the selected machine");
        assert_eq!(
            admission.configuration.to_wire_json(),
            serde_json::json!({
                "$": "record",
                "fields": [{
                    "name": "seed",
                    "value": { "$": "Int", "value": LARGE },
                }],
            })
        );

        let identity = deployment_identity(
            &program,
            &deployment,
            &admission.configuration,
            &admission.ports,
            "",
            Some(TEST_PROVIDER_JS),
        )
        .unwrap();
        let play: serde_json::Value = serde_json::from_str(&super::play_config(
            &deployment,
            &identity,
            &admission.configuration,
            admission.ports.clone(),
            None,
        ))
        .unwrap();
        assert_eq!(
            play["configuration"],
            admission.configuration.to_wire_json()
        );

        let changed_source = source.replace(LARGE, "900719925474099312345678901234567891");
        let changed_deployment = parse_host_manifest(&changed_source).unwrap();
        let changed = validate_deployment(&program, &changed_deployment).unwrap();
        let changed_identity = deployment_identity(
            &program,
            &changed_deployment,
            &changed.configuration,
            &changed.ports,
            "",
            Some(TEST_PROVIDER_JS),
        )
        .unwrap();
        assert_ne!(changed_identity.deployment_hash, identity.deployment_hash);

        let missing = source
            .lines()
            .filter(|line| !line.starts_with("configuration = "))
            .collect::<Vec<_>>()
            .join("\n");
        let missing = parse_host_manifest(&missing).unwrap();
        let (code, message) = validate_deployment(&program, &missing).unwrap_err();
        assert_eq!(code, "R3014");
        assert!(message.contains("must provide `configuration`"));

        let noncanonical = source.replace(r#"{"$":"record""#, r#"{"$": "record""#);
        let noncanonical = parse_host_manifest(&noncanonical).unwrap();
        let (code, message) = validate_deployment(&program, &noncanonical).unwrap_err();
        assert_eq!(code, "R3014");
        assert!(message.contains("must be canonical exact tagged Uhura JSON"));

        let wrong_type = source.replace(r#""$":"Int""#, r#""$":"Text""#);
        let wrong_type = parse_host_manifest(&wrong_type).unwrap();
        let (code, message) = validate_deployment(&program, &wrong_type).unwrap_err();
        assert_eq!(code, "R3014");
        assert!(message.contains("does not satisfy"));
    }

    #[test]
    fn uhura_host_admits_imported_contract_instances_without_domain_knowledge() {
        const DOMAIN: &str = "vendor.fulfillment@1";
        const WRAPPER: &str = "app.return_desk.wrapper@1::Wrapper";

        let program = uhura_program_with_adapter_domain(DOMAIN, WRAPPER);

        let deployment = parse_host_manifest(
            r#"
[entry.wrapper]
machine = "app.return_desk.wrapper@1::Wrapper"
lifetime = "application-session"

[entry.wrapper.ports]
router = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.wrapper.provider]
module = "provider.mjs"
"#,
        )
        .unwrap();
        let admission = validate_deployment(&program, &deployment)
            .expect("wrapper imports the exact sealed adapter boundary types");
        let admitted = admission.ports.clone();

        let orders_binding = admitted
            .iter()
            .find(|binding| binding["port"] == "orders")
            .unwrap();
        assert_eq!(
            orders_binding["contractInstanceHash"],
            program.machine_program.machines[WRAPPER]
                .ports
                .iter()
                .find(|port| port.name == "orders")
                .unwrap()
                .contract_instance
                .as_ref()
                .unwrap()
                .instance_hash()
        );
        let returns_binding = admitted
            .iter()
            .find(|binding| binding["port"] == "returns")
            .unwrap();
        assert_eq!(
            returns_binding["contractInstanceHash"],
            program.machine_program.machines[WRAPPER]
                .ports
                .iter()
                .find(|port| port.name == "returns")
                .unwrap()
                .contract_instance
                .as_ref()
                .unwrap()
                .instance_hash()
        );

        let identity = deployment_identity(
            &program,
            &deployment,
            &admission.configuration,
            &admitted,
            "",
            Some(TEST_PROVIDER_JS),
        )
        .unwrap();
        let play: serde_json::Value = serde_json::from_str(&super::play_config(
            &deployment,
            &identity,
            &admission.configuration,
            admitted.clone(),
            None,
        ))
        .unwrap();
        assert_eq!(play["ports"], serde_json::Value::Array(admitted));
    }

    #[test]
    fn uhura_host_admission_checks_instantiated_types_configuration_and_codecs() {
        use uhura_port::{CanonicalJson, TypeRef as PortTypeRef, observation_instance};

        let source = r#"
[entry.return-desk]
machine = "app.return_desk.machine@1::ReturnDesk"
lifetime = "application-session"

[entry.return-desk.ports]
router = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"
"#;
        let deployment = parse_host_manifest(source).unwrap();

        let mut wrong_type = uhura_program_with_a0_ports();
        let orders = wrong_type
            .machine_program
            .machines
            .get_mut("app.return_desk.machine@1::ReturnDesk")
            .unwrap()
            .ports
            .iter_mut()
            .find(|port| port.name == "orders")
            .unwrap();
        let text_observation = observation_instance(PortTypeRef::new("Text").unwrap()).unwrap();
        orders.contract_instance = Some(text_observation.clone());
        orders.type_arguments = vec![TypeRef::Text];
        orders.receive[0].fields[0].1 = TypeRef::Text;
        orders.contract = text_observation.identity.to_string();
        orders.contract_hash = text_observation.content_hash;
        validate_deployment(&wrong_type, &deployment)
            .expect("a generic application provider is agnostic to app-owned wire types");

        let mut wrong_configuration = uhura_program_with_a0_ports();
        let router = wrong_configuration
            .machine_program
            .machines
            .get_mut("app.return_desk.machine@1::ReturnDesk")
            .unwrap()
            .ports
            .iter_mut()
            .find(|port| port.name == "router")
            .unwrap();
        router.contract_instance.as_mut().unwrap().configuration =
            CanonicalJson::new(serde_json::json!({ "forged": true })).unwrap();
        let (code, message) = validate_deployment(&wrong_configuration, &deployment).unwrap_err();
        assert_eq!(code, "R3015");
        assert!(
            message.contains("invalid resolved route-table configuration"),
            "{message}"
        );

        let mut wrong_codec = uhura_program_with_a0_ports();
        let orders = wrong_codec
            .machine_program
            .machines
            .get_mut("app.return_desk.machine@1::ReturnDesk")
            .unwrap()
            .ports
            .iter_mut()
            .find(|port| port.name == "orders")
            .unwrap();
        orders.contract_instance.as_mut().unwrap().codecs[0].semantic_id =
            "test.incompatible-codec@1".into();
        let (code, message) = validate_deployment(&wrong_codec, &deployment).unwrap_err();
        assert_eq!(code, "R3015");
        assert!(message.contains("contract instance differs"));
    }

    #[test]
    fn uhura_host_admission_rejects_multiple_web_history_owners() {
        const MACHINE: &str = "app.return_desk.machine@1::ReturnDesk";

        let mut program = uhura_program_with_a0_ports();
        let machine = program.machine_program.machines.get_mut(MACHINE).unwrap();
        let mut second_router = machine
            .ports
            .iter()
            .find(|port| port.name == "router")
            .unwrap()
            .clone();
        second_router.name = "router_backup".into();
        machine.ports.push(second_router);
        program.freeze_program_hashes();

        let deployment = parse_host_manifest(
            r#"
[entry.return-desk]
machine = "app.return_desk.machine@1::ReturnDesk"
lifetime = "application-session"

[entry.return-desk.ports]
router = "web.history"
router_backup = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"
"#,
        )
        .unwrap();

        let (code, message) = validate_deployment(&program, &deployment).unwrap_err();
        assert_eq!(code, "R3015");
        assert_eq!(
            message,
            "sealed host adapter capability `web.history` may bind at most one machine port; found bindings [router, router_backup]"
        );
    }

    #[test]
    fn deployment_identity_covers_manifest_and_admitted_contracts() {
        let source = r#"
[entry.return-desk]
machine = "app.return_desk.machine@1::ReturnDesk"
lifetime = "application-session"

[entry.return-desk.ports]
router = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"
"#;
        let program = uhura_program_with_a0_ports();
        let deployment = parse_host_manifest(source).unwrap();
        let admission = validate_deployment(&program, &deployment).unwrap();
        let identity = deployment_identity(
            &program,
            &deployment,
            &admission.configuration,
            &admission.ports,
            "",
            Some(TEST_PROVIDER_JS),
        )
        .unwrap();
        assert_eq!(identity.protocol, MACHINE_PROGRAM_ID_PROTOCOL);
        assert_eq!(identity.machine_program_hash.len(), 64);
        assert_eq!(identity.deployment_hash.len(), 64);
        assert_eq!(identity.presentation_hash, None);
        assert_eq!(identity.evidence_hash, None);

        let mut reordered = admission.ports.clone();
        reordered.reverse();
        assert_eq!(
            deployment_identity(
                &program,
                &deployment,
                &admission.configuration,
                &reordered,
                "",
                Some(TEST_PROVIDER_JS),
            )
            .unwrap()
            .deployment_hash,
            identity.deployment_hash,
        );

        let mut renamed = deployment.clone();
        renamed.entry = "another-entry".into();
        assert_ne!(
            deployment_identity(
                &program,
                &renamed,
                &admission.configuration,
                &admission.ports,
                "",
                Some(TEST_PROVIDER_JS),
            )
            .unwrap()
            .deployment_hash,
            identity.deployment_hash,
        );

        let mut changed_contract = admission.ports.clone();
        changed_contract[0]["contractHash"] = serde_json::Value::String("11".repeat(32));
        assert_ne!(
            deployment_identity(
                &program,
                &deployment,
                &admission.configuration,
                &changed_contract,
                "",
                Some(TEST_PROVIDER_JS),
            )
            .unwrap()
            .deployment_hash,
            identity.deployment_hash,
        );
    }

    #[test]
    fn deployment_identity_excludes_resource_paths_but_covers_content_and_config() {
        let source = r#"
[entry.return-desk]
machine = "app.return_desk.machine@1::ReturnDesk"
lifetime = "application-session"
stylesheet = "styles/theme.css"

[entry.return-desk.ports]
router = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"

[entry.return-desk.provider.config]
endpoint = "https://one.example"
"#;
        let mut program = uhura_program_with_a0_ports();
        program.freeze_program_hashes();
        let deployment = parse_host_manifest(source).unwrap();
        let admission = validate_deployment(&program, &deployment).unwrap();
        let identity = deployment_identity(
            &program,
            &deployment,
            &admission.configuration,
            &admission.ports,
            "body { color: red; }",
            Some("export const provider = 1;"),
        )
        .unwrap();

        let mut moved = deployment.clone();
        moved.stylesheet = Some("relocated/theme.css".into());
        moved.provider.as_mut().unwrap().module = "relocated/provider.mjs".into();
        assert_eq!(
            deployment_identity(
                &program,
                &moved,
                &admission.configuration,
                &admission.ports,
                "body { color: red; }",
                Some("export const provider = 1;"),
            )
            .unwrap()
            .deployment_hash,
            identity.deployment_hash
        );

        assert_ne!(
            deployment_identity(
                &program,
                &deployment,
                &admission.configuration,
                &admission.ports,
                "body { color: blue; }",
                Some("export const provider = 1;"),
            )
            .unwrap()
            .deployment_hash,
            identity.deployment_hash
        );
        assert_ne!(
            deployment_identity(
                &program,
                &deployment,
                &admission.configuration,
                &admission.ports,
                "body { color: red; }",
                Some("export const provider = 2;"),
            )
            .unwrap()
            .deployment_hash,
            identity.deployment_hash
        );

        let mut reconfigured = deployment;
        reconfigured.provider.as_mut().unwrap().config =
            serde_json::json!({"endpoint": "https://two.example"});
        assert_ne!(
            deployment_identity(
                &program,
                &reconfigured,
                &admission.configuration,
                &admission.ports,
                "body { color: red; }",
                Some("export const provider = 1;"),
            )
            .unwrap()
            .deployment_hash,
            identity.deployment_hash
        );
    }

    #[test]
    fn deployment_identity_admits_only_the_current_language_protocol_pair() {
        let source = r#"
[entry.return-desk]
machine = "app.return_desk.machine@1::ReturnDesk"
lifetime = "application-session"

[entry.return-desk.ports]
router = "web.history"
orders = "app.provider"
returns = "app.provider"

[entry.return-desk.provider]
module = "provider.mjs"
"#;
        let mut program = uhura_program_with_a0_ports();
        let deployment = parse_host_manifest(source).unwrap();
        let admission = validate_deployment(&program, &deployment).unwrap();

        let current = deployment_identity(
            &program,
            &deployment,
            &admission.configuration,
            &admission.ports,
            "",
            Some(TEST_PROVIDER_JS),
        )
        .unwrap();
        assert_eq!(current.protocol, MACHINE_PROGRAM_ID_PROTOCOL);

        program.machine_program.identity_protocol = "uhura-unrecognized-identity/9".into();
        let error = deployment_identity(
            &program,
            &deployment,
            &admission.configuration,
            &admission.ports,
            "",
            Some(TEST_PROVIDER_JS),
        )
        .unwrap_err();
        assert!(error.contains("unsupported Uhura machine identity protocol"));
    }

    #[test]
    fn uhura_play_serves_an_explicit_empty_icon_manifest() {
        let candidate = ClientCandidate {
            revision: 1,
            source_fingerprint: ProjectSourceFingerprint::default(),
            source_revision_id: "test-source-revision".into(),
            editor: Ok(uhura_editor_artifact(1)),
            play: Ok(uhura_good_build()),
            checked_routes: Some(Vec::new()),
        };
        let host = Host::new(test_web_assets(), candidate).unwrap().0;
        let response = host.route(RouteRequest {
            method: RequestMethod::Get,
            url: "/api/play/icon-fonts.json",
        });
        assert_eq!(response.status, 200);
        let manifest: serde_json::Value =
            serde_json::from_slice(&response_bytes(response)).unwrap();
        assert_eq!(manifest["protocol"], "uhura-icon-fonts/0");
        assert_eq!(manifest["generation"], 1);
        assert_eq!(manifest["default"], serde_json::Value::Null);
        assert_eq!(manifest["families"], serde_json::json!({}));
    }

    #[test]
    fn uhura_corpus_uses_one_canonical_machine_build() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("uhura-host-{}-{unique}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("app.uhura"),
            r#"pub machine App {
  events {
    Noop,
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = 0,
  }

  observe {
    count,
  }

  on Noop {
    Accepted
  }
}
"#,
        )
        .unwrap();
        fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.app"
version = 1
language = "0.4"

[modules]
app = "app.uhura"
"#,
        )
        .unwrap();
        fs::write(
            root.join("host.toml"),
            r#"[entry.app]
machine = "crate::App"
lifetime = "application-session"

[entry.app.ports]
"#,
        )
        .unwrap();

        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        assert!(
            candidate.summary().editor_current,
            "editor diagnostics: {:#}",
            candidate.diagnostics().editor
        );
        assert!(
            candidate.summary().play_ok,
            "play diagnostics: {:#}",
            candidate.diagnostics().play
        );
        let (host, _) = Host::new(test_web_assets(), candidate).unwrap();
        let config: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/play/config.json",
            })))
            .unwrap();
        assert_eq!(config["protocol"], "uhura-play-config/1");
        assert!(config.get("runtime").is_none());
        assert_eq!(config["entry"], "app");
        assert_eq!(config["machine"], "test.app@1::App");
        assert_eq!(config["instance"], "entry/app");
        assert_eq!(config["configuration"], serde_json::json!({ "$": "unit" }));
        assert_eq!(config["ports"], serde_json::json!([]));

        fs::write(root.join("app.uhura"), "this is not valid Uhura").unwrap();
        let rejected = super::build_candidate(&crate::source::capture_project_snapshot(&root), 2);
        assert!(!rejected.summary().editor_current);
        assert!(!rejected.summary().play_ok);
        assert_eq!(
            rejected.diagnostics().play["diagnostics"][0]["code"],
            "R1001"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uhura_non_unit_configuration_reaches_play_exactly_and_preflights_genesis() {
        const LARGE: &str = "900719925474099312345678901234567890";
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-configured-host-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("app.uhura"),
            r#"pub machine Configured {
  config {
    initial: Int,
  }

  require initial > 0;

  events {
    Noop,
  }

  outcomes {
    commit Accepted,
  }

  state {
    value: Int = initial,
  }

  invariant value > 0;

  observe {
    value,
  }

  on Noop {
    Accepted
  }
}
"#,
        )
        .unwrap();
        fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.configured"
version = 1
language = "0.4"

[modules]
configured = "app.uhura"
"#,
        )
        .unwrap();
        let valid_host = format!(
            r#"[entry.configured]
machine = "crate::Configured"
lifetime = "application-session"
configuration = '{{"$":"record","fields":[{{"name":"initial","value":{{"$":"Int","value":"{LARGE}"}}}}]}}'

[entry.configured.ports]
"#
        );
        fs::write(root.join("host.toml"), &valid_host).unwrap();

        let candidate = super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        assert!(candidate.summary().editor_current);
        assert!(candidate.summary().play_ok);
        let (host, _) = Host::new(test_web_assets(), candidate).unwrap();
        let config: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/play/config.json",
            })))
            .unwrap();
        assert_eq!(
            config["configuration"],
            serde_json::json!({
                "$": "record",
                "fields": [{
                    "name": "initial",
                    "value": { "$": "Int", "value": LARGE },
                }],
            })
        );
        assert_eq!(
            config["instance"], "entry/configured",
            "the validation-only genesis identity must not leak into Play"
        );

        fs::write(root.join("host.toml"), valid_host.replace(LARGE, "-1")).unwrap();
        let rejected = super::build_candidate(&crate::source::capture_project_snapshot(&root), 2);
        assert!(
            rejected.summary().editor_current,
            "deployment rejection must not discard the checked graph"
        );
        assert!(!rejected.summary().play_ok);
        assert!(
            rejected.diagnostics().play["diagnostics"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("cannot create Uhura machine genesis"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uhura_program_corpus_without_host_is_a_current_graph_only_editor() {
        let root = tool_root().join("examples/programs/answers/uhura-0.4");
        let candidate = super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        let summary = candidate.summary();
        assert!(summary.editor_current);
        assert!(!summary.play_ok);
        assert_eq!(summary.preview_count, Some(0));

        let editor = accepted_editor_render(&candidate);
        assert_eq!(editor["freshness"], "current");
        assert_eq!(editor["revision"], 1);
        assert_eq!(
            editor["machine"]["identityProtocol"],
            MACHINE_PROGRAM_ID_PROTOCOL
        );
        assert_eq!(
            editor["machine"]["deployment"],
            serde_json::Value::Null,
            "deployment authority must remain absent"
        );
        assert!(
            editor["machine"]["interactionGraph"]["nodes"]
                .as_array()
                .is_some_and(|nodes| nodes.len() >= 3)
        );
        assert_eq!(
            candidate.diagnostics().editor["diagnostics"][0]["rule"],
            "uhura/host-manifest"
        );
        assert_eq!(
            candidate.diagnostics().play["diagnostics"][0]["rule"],
            "uhura/host-manifest"
        );
    }

    #[test]
    fn uhura_a0_host_failures_preserve_twelve_static_editor_previews() {
        for (label, host) in [
            (
                "invalid-adapter",
                r#"[entry.return-desk]
machine = "app.return_desk.machine@1::ReturnDesk"
presentation = "app.return_desk.web@1::ReturnDeskWeb"
lifetime = "application-session"

[entry.return-desk.ports]
router = "web.missing"
orders = "return-desk.orders"
returns = "return-desk.returns"
"#,
            ),
            ("malformed-host", "[entry.return-desk\nmachine = true"),
        ] {
            let root = copy_a0_fixture(label);
            fs::write(root.join("host.toml"), host).unwrap();
            let candidate =
                super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
            let summary = candidate.summary();
            assert!(
                summary.editor_current,
                "{label}: {:?}",
                candidate.diagnostics()
            );
            assert!(!summary.play_ok);
            assert_eq!(summary.preview_count, Some(12));
            assert_eq!(summary.replay_derived_count, Some(11));

            let editor = accepted_editor_render(&candidate);
            assert_eq!(editor["previews"].as_array().unwrap().len(), 12);
            assert_eq!(editor["machine"]["evidence"]["passed"], true);
            assert!(
                editor["machine"]["interactionGraph"]["nodes"]
                    .as_array()
                    .is_some_and(|nodes| !nodes.is_empty())
            );
            assert_eq!(
                editor["machine"]["deployment"],
                serde_json::Value::Null,
                "{label}: rejected deployment authority must remain absent"
            );
            assert!(
                candidate.diagnostics().editor["diagnostics"]
                    .as_array()
                    .is_some_and(|diagnostics| !diagnostics.is_empty())
            );
            assert!(
                candidate.diagnostics().play["diagnostics"]
                    .as_array()
                    .is_some_and(|diagnostics| !diagnostics.is_empty())
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn uhura_a0_navigation_nodes_are_preview_backed_and_deterministic() {
        let root = tool_root().join("examples/applications/a0-return-desk/answers/uhura-0.4");
        let candidate = super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        let artifact = candidate
            .editor
            .as_ref()
            .unwrap_or_else(|rejection| panic!("A0 Editor rejection: {:?}", rejection.diagnostics));
        let graph = &artifact.render.interaction_graph;
        let preview_ids = artifact
            .render
            .previews
            .iter()
            .map(|preview| preview.id.as_str())
            .collect::<BTreeSet<_>>();
        let node_ids = graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<BTreeSet<_>>();
        let route_nodes = graph
            .nodes
            .iter()
            .filter(|node| node.id.starts_with("preview:"))
            .collect::<Vec<_>>();

        assert!(
            route_nodes.len() >= 3,
            "Flow, Order, and Receipt must be visible despite unbound examples"
        );
        assert!(route_nodes.iter().all(|node| {
            node.id
                .strip_prefix("preview:")
                .is_some_and(|preview| preview_ids.contains(preview))
        }));
        assert!(
            graph
                .nodes
                .windows(2)
                .all(|nodes| nodes[0].id <= nodes[1].id)
        );

        let navigate = graph
            .edges
            .iter()
            .filter(|edge| edge.kind == super::ApplicationEdgeKind::Navigate)
            .collect::<Vec<_>>();
        assert!(!navigate.is_empty());
        assert!(navigate.iter().all(|edge| {
            edge.from.starts_with("preview:")
                && edge.to.starts_with("preview:")
                && node_ids.contains(edge.from.as_str())
                && node_ids.contains(edge.to.as_str())
        }));
        let events = navigate
            .iter()
            .map(|edge| edge.event.as_str())
            .collect::<BTreeSet<_>>();
        assert!(events.contains("GoToStep"));
        assert!(events.contains("FollowOrderLink"));
        assert!(events.contains("FollowReceiptLink"));
        for (index, edge) in graph.edges.iter().enumerate() {
            assert_eq!(edge.id, format!("application/edge/{index}"));
        }
        assert!(graph.edges.windows(2).all(|edges| {
            (
                &edges[0].from,
                &edges[0].to,
                &edges[0].event,
                super::application_edge_rank(edges[0].kind),
            ) <= (
                &edges[1].from,
                &edges[1].to,
                &edges[1].event,
                super::application_edge_rank(edges[1].kind),
            )
        }));

        let inspection: serde_json::Value = serde_json::from_str(
            &candidate
                .play
                .as_ref()
                .expect("A0 Play admission")
                .inspect_json,
        )
        .expect("A0 Play inspection JSON");
        let graph_source_paths = inspection_graph_source_paths(&inspection);
        assert!(graph_source_paths.contains("machine.uhura"));
        assert!(graph_source_paths.contains("ui.uhura"));
        assert!(!graph_source_paths.contains("<resolved-project>"));
    }

    #[test]
    fn uhura_editor_replay_is_the_direct_delta_for_chained_and_snapshot_origin_pins() {
        fn preview_named<'a>(previews: &'a [Preview], local_name: &str) -> &'a Preview {
            previews
                .iter()
                .find(|preview| {
                    preview.identity.example == local_name
                        || preview
                            .identity
                            .example
                            .ends_with(&format!("::{local_name}"))
                })
                .unwrap_or_else(|| panic!("missing `{local_name}` preview"))
        }

        let root = tool_root().join("examples/applications/a0-return-desk/answers/uhura-0.4");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let checked = super::check_project_snapshot(&snapshot).expect("checked A0 project");
        let editor = super::build_editor(1, &checked, None, serde_json::Value::Null).unwrap();
        let previews = &editor.render.previews;

        let waiting = preview_named(previews, "waiting_at_items");
        let incomplete = preview_named(previews, "incomplete_items");
        let complete = preview_named(previews, "complete_items");
        assert_eq!(waiting.from, None);
        assert!(waiting.replay.is_empty());
        assert!(!waiting.derived);
        assert_eq!(
            incomplete.from.as_deref(),
            Some(waiting.identity.example.as_str())
        );
        assert_eq!(
            complete.from.as_deref(),
            Some(incomplete.identity.example.as_str())
        );

        for (parent, child) in [(waiting, incomplete), (incomplete, complete)] {
            let parent_reference =
                super::preview_reference(parent).expect("parent evidence reference");
            let child_reference =
                super::preview_reference(child).expect("child evidence reference");
            assert_eq!(parent_reference.scenario, child_reference.scenario);
            let parent_pin = super::pin_artifact(&checked.evidence, &parent_reference).unwrap();
            let child_pin = super::pin_artifact(&checked.evidence, &child_reference).unwrap();
            let expected_delta =
                child_pin.snapshot.next_sequence - parent_pin.snapshot.next_sequence;
            assert_eq!(
                u64::try_from(child.replay.len()).unwrap(),
                expected_delta,
                "same-scenario replay must contain only receipts after its direct parent"
            );
            assert!(
                child
                    .evidence
                    .as_ref()
                    .unwrap()
                    .scenario_receipt_log
                    .as_ref()
                    .unwrap()["receipts"]
                    .as_array()
                    .unwrap()
                    .len()
                    > child.replay.len(),
                "the evidence receipt prefix remains complete even when replay is an edge delta"
            );
        }

        let review = preview_named(previews, "review");
        let retry = preview_named(previews, "retryable_unavailability");
        assert_ne!(
            review.evidence.as_ref().unwrap().scenario,
            retry.evidence.as_ref().unwrap().scenario
        );
        assert_eq!(
            retry.from.as_deref(),
            Some(review.identity.example.as_str()),
            "the nearest visible ancestor before a snapshot-origin pin crosses scenario ids"
        );

        let review_reference = super::preview_reference(review).expect("review evidence reference");
        let retry_reference = super::preview_reference(retry).expect("retry evidence reference");
        let review_pin = super::pin_artifact(&checked.evidence, &review_reference).unwrap();
        let retry_pin = super::pin_artifact(&checked.evidence, &retry_reference).unwrap();
        let retry_scenario = &checked.program.evidence.scenarios[&retry_reference.scenario];
        let uhura_core::ir::ScenarioOrigin::Snapshot { reference: origin } = &retry_scenario.origin
        else {
            panic!("retry scenario must restore a published snapshot");
        };
        let origin_pin = super::pin_artifact(&checked.evidence, origin).unwrap();
        let ancestor_receipts = checked
            .evidence
            .scenarios
            .iter()
            .find(|scenario| scenario.scenario == review_reference.scenario)
            .unwrap()
            .receipts
            .iter()
            .filter(|receipt| {
                review_pin.snapshot.next_sequence <= receipt.sequence
                    && receipt.sequence < origin_pin.snapshot.next_sequence
            })
            .count();
        let resumed_receipts = checked
            .evidence
            .scenarios
            .iter()
            .find(|scenario| scenario.scenario == retry_reference.scenario)
            .unwrap()
            .receipts
            .iter()
            .filter(|receipt| {
                origin_pin.snapshot.next_sequence <= receipt.sequence
                    && receipt.sequence < retry_pin.snapshot.next_sequence
            })
            .count();
        assert_eq!(retry.replay.len(), ancestor_receipts + resumed_receipts);
        assert_eq!(
            retry
                .evidence
                .as_ref()
                .unwrap()
                .scenario_receipt_log
                .as_ref()
                .unwrap()["receipts"]
                .as_array()
                .unwrap()
                .len(),
            resumed_receipts,
            "scenario evidence remains its full local prefix, independent of connector replay"
        );
        assert!(
            retry.replay.len() > resumed_receipts,
            "the cross-scenario edge includes the missing ancestor suffix exactly once"
        );
    }

    #[test]
    fn structural_annotations_anchor_rendered_if_and_each_content() {
        let root = copy_a0_fixture("structural-annotations");
        let ui_path = root.join("ui.uhura");
        let source = fs::read_to_string(&ui_path).unwrap();
        let annotated = source
            .replacen(
                "    {#if view.page is Page::NoLocation}",
                "    <!-- @rationale The active route branch. -->\n    {#if view.page is Page::NoLocation}",
                1,
            )
            .replacen(
                "              {#each order.lines.entries_by_key() as (id, line) (id)}",
                "              <!-- @review-note Every visible order line. -->\n              {#each order.lines.entries_by_key() as (id, line) (id)}",
                1,
            );
        assert_ne!(annotated, source);
        fs::write(&ui_path, annotated).unwrap();

        let candidate = super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        assert!(
            candidate.summary().editor_current,
            "{:?}",
            candidate.diagnostics()
        );
        let editor = accepted_editor_render(&candidate);
        let entries = editor["authoring"]["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        let targets = editor["authoring"]["targets"].as_array().unwrap();
        let previews = editor["previews"].as_array().unwrap();
        for expected_class in ["if-block", "each-block"] {
            let target = targets
                .iter()
                .find(|target| {
                    target["class"] == expected_class
                        && entries
                            .iter()
                            .any(|entry| entry["targetId"] == target["id"])
                })
                .unwrap_or_else(|| panic!("missing annotated `{expected_class}` target"));
            assert!(previews.iter().any(|preview| {
                preview["provenance"]["occurrences"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|occurrence| {
                        occurrence["targetId"] == target["id"]
                            && occurrence["anchors"]
                                .as_array()
                                .is_some_and(|anchors| !anchors.is_empty())
                    })
            }));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn annotation_only_source_changes_preserve_documents_and_refresh_projection_sources() {
        let root = copy_a0_fixture("annotation-only-update");
        let baseline_candidate =
            super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        assert!(
            baseline_candidate.summary().editor_current,
            "{:?}",
            baseline_candidate.diagnostics()
        );
        let baseline = accepted_editor_render(&baseline_candidate);
        assert!(
            baseline["authoring"]["entries"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );

        let ui_path = root.join("ui.uhura");
        let source = fs::read_to_string(&ui_path).unwrap();
        let annotated = source.replacen(
            "  <main aria-label=\"Return desk\">",
            "  <!-- @annotation Authoring-only prose with a different byte width. -->\n  <main aria-label=\"Return desk\">",
            1,
        );
        assert_ne!(annotated, source);
        fs::write(&ui_path, annotated).unwrap();

        let revised_candidate =
            super::build_candidate(&crate::source::capture_project_snapshot(&root), 2);
        assert!(
            revised_candidate.summary().editor_current,
            "{:?}",
            revised_candidate.diagnostics()
        );
        let revised = accepted_editor_render(&revised_candidate);
        assert_eq!(revised["authoring"]["entries"].as_array().unwrap().len(), 1);

        let baseline_previews = baseline["previews"].as_array().unwrap();
        let revised_previews = revised["previews"].as_array().unwrap();
        assert_eq!(baseline_previews.len(), revised_previews.len());
        let mut refreshed_source_sidecar = false;
        for baseline_preview in baseline_previews {
            let id = baseline_preview["id"].as_str().unwrap();
            let revised_preview = revised_previews
                .iter()
                .find(|preview| preview["id"] == id)
                .expect("annotation-only changes retain preview identity");
            assert_eq!(
                baseline_preview["content"]["value"]["document"],
                revised_preview["content"]["value"]["document"],
                "annotation-only changes retain renderer-semantic content for `{id}`"
            );
            refreshed_source_sidecar |= baseline_preview["content"]["value"]["sources"]
                != revised_preview["content"]["value"]["sources"];
        }
        assert!(
            refreshed_source_sidecar,
            "annotation byte offsets must refresh the projection source sidecar"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uhura_a0_evidence_failure_is_editor_diagnostic_not_play_admission() {
        let root = copy_a0_fixture("evidence-failure");
        let evidence_path = root.join("evidence/conformance.uhura");
        let evidence = fs::read_to_string(&evidence_path).unwrap();
        let broken = evidence.replacen(
            "replay_accepted_suffix::replay_final",
            "canonical::waiting_at_items",
            1,
        );
        assert_ne!(broken, evidence);
        fs::write(&evidence_path, broken).unwrap();

        let candidate = super::build_candidate(&crate::source::capture_project_snapshot(&root), 1);
        let summary = candidate.summary();
        assert!(summary.editor_current, "{:?}", candidate.diagnostics());
        assert!(summary.play_ok, "{:?}", candidate.diagnostics());
        assert_eq!(summary.preview_count, Some(12));
        assert_eq!(
            candidate.diagnostics().editor["diagnostics"][0]["code"],
            "R3013"
        );
        assert!(candidate.diagnostics().play.is_null());

        let editor = accepted_editor_render(&candidate);
        assert_eq!(editor["machine"]["evidence"]["passed"], false);
        assert!(
            editor["machine"]["evidence"]["failureCount"]
                .as_u64()
                .is_some_and(|failures| failures > 0)
        );
        assert_eq!(editor["previews"].as_array().unwrap().len(), 12);
        assert!(
            editor["machine"]["deployment"]["deploymentHash"]
                .as_str()
                .is_some()
        );

        let play = candidate
            .play
            .as_ref()
            .expect("evidence does not reject Play");
        let inspection: serde_json::Value = serde_json::from_str(&play.inspect_json).unwrap();
        assert_eq!(inspection["evidence"]["passed"], false);
        assert!(
            inspection["evidence"]["failureCount"]
                .as_u64()
                .is_some_and(|failures| failures > 0)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uhura_a0_candidate_publishes_evidence_views_and_play_artifacts() {
        fn render_keys(nodes: &[serde_json::Value], keys: &mut BTreeSet<String>) {
            for node in nodes {
                keys.insert(node["key"].as_str().unwrap().to_string());
                if let Some(children) = node.get("children").and_then(serde_json::Value::as_array) {
                    render_keys(children, keys);
                }
            }
        }

        let root = tool_root().join("examples/applications/a0-return-desk/answers/uhura-0.4");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        let summary = candidate.summary();
        assert!(
            summary.editor_current && summary.play_ok,
            "Uhura 0.4 A0 diagnostics: {:?}",
            candidate.diagnostics()
        );
        assert_eq!(summary.preview_count, Some(12));
        assert_eq!(summary.replay_derived_count, Some(11));

        let (host, _) = Host::new(test_web_assets(), candidate).unwrap();
        let editor: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/editor/state",
            })))
            .unwrap();
        assert_eq!(editor["protocol"], "uhura-editor-state/5");
        let render = &editor["render"];
        let machine = &render["machine"];
        let source_inventory = machine["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|source| {
                (
                    source["path"].as_str().unwrap(),
                    source["bytes"].as_u64().unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(render["previews"].as_array().unwrap().len(), 12);
        let authoring_targets = render["authoring"]["targets"].as_array().unwrap();
        assert!(
            !authoring_targets.is_empty(),
            "machine projections must retain source-navigation targets"
        );
        assert!(
            render["authoring"]["entries"]
                .as_array()
                .unwrap()
                .is_empty(),
            "source navigation must not invent authored annotations or documentation"
        );
        let authoring_target_ids = authoring_targets
            .iter()
            .map(|target| target["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            authoring_target_ids.len(),
            authoring_targets.len(),
            "semantic source identities must be deduplicated"
        );
        for target in authoring_targets {
            assert_eq!(target["class"], "ui-element");
            assert_eq!(target["owner"]["kind"], "page");
            assert!(
                target["owner"]["name"]
                    .as_str()
                    .is_some_and(|name| !name.is_empty())
            );
            let bytes = source_inventory[target["file"].as_str().unwrap()];
            assert!(target["span"]["offset"].as_u64().unwrap() <= bytes);
            assert!(
                target["span"]["offset"].as_u64().unwrap()
                    + target["span"]["len"].as_u64().unwrap()
                    <= bytes
            );
            assert!(target["span"]["start"]["line"].as_u64().unwrap() > 0);
            assert!(target["span"]["start"]["col"].as_u64().unwrap() > 0);
            assert!(target["span"]["end"]["line"].as_u64().unwrap() > 0);
            assert!(target["span"]["end"]["col"].as_u64().unwrap() > 0);
        }
        assert_eq!(machine["sources"].as_array().unwrap().len(), 4);
        assert_eq!(machine["provenance"]["protocol"], "uhura-provenance/0");
        assert_eq!(
            machine["interactionGraph"]["protocol"],
            "uhura-interaction-graph/0"
        );
        assert_eq!(
            machine["graphSources"]["protocol"],
            "uhura-interaction-graph-provenance/0"
        );
        let graph_nodes = machine["interactionGraph"]["nodes"].as_array().unwrap();
        let graph_edges = machine["interactionGraph"]["edges"].as_array().unwrap();
        assert!(!graph_nodes.is_empty());
        assert!(!graph_edges.is_empty());
        assert!(
            machine["graphSources"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["sources"]
                    .as_array()
                    .is_some_and(|sources| !sources.is_empty()))
        );
        for preview in render["previews"].as_array().unwrap() {
            assert_eq!(preview["content"]["kind"], "projection");
            assert_eq!(
                preview["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|field| field["name"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["configuration", "state", "observation"]
            );
            assert!(preview["data"].as_array().unwrap().iter().all(|field| {
                field["group"] == "provided-data"
                    && field["status"] == "ready"
                    && field["key"].is_null()
                    && field["source"].is_null()
                    && field["value"].is_object()
            }));
            for source in [
                &preview["evidence"]["sources"]["registration"],
                &preview["evidence"]["sources"]["pin"],
            ] {
                let bytes = source_inventory[source["path"].as_str().unwrap()];
                assert!(source["start"].as_u64().unwrap() > 0);
                assert!(source["end"].as_u64().unwrap() <= bytes);
            }
            let projection = &preview["content"]["value"];
            assert_eq!(projection["document"]["protocol"], "uhura-view/1");
            assert!(projection["document"]["sequence"].is_string());
            assert_eq!(
                preview["evidence"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from(["pin", "scenario", "sourceId", "sources"]),
                "Editor preview evidence is an identity and source-navigation record"
            );
            assert_eq!(
                projection["sources"]["protocol"],
                "uhura-projection-sources/0"
            );
            assert_eq!(
                projection["sources"]["presentation"],
                projection["document"]["presentation"]
            );
            let mut rendered = BTreeSet::new();
            render_keys(
                projection["document"]["nodes"].as_array().unwrap(),
                &mut rendered,
            );
            let projected = projection["sources"]["nodes"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>();
            assert_eq!(rendered, projected);
            let occurrences = preview["provenance"]["occurrences"].as_array().unwrap();
            assert!(
                !occurrences.is_empty(),
                "every non-empty A0 projection must have source-backed occurrences"
            );
            let anchored = occurrences
                .iter()
                .flat_map(|occurrence| {
                    assert!(
                        authoring_target_ids.contains(occurrence["targetId"].as_str().unwrap()),
                        "every occurrence target must resolve"
                    );
                    occurrence["anchors"].as_array().unwrap()
                })
                .map(|anchor| anchor.as_str().unwrap().to_string())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                anchored, rendered,
                "projection provenance must cover every rendered semantic key exactly"
            );
            for source in projection["sources"]["nodes"].as_object().unwrap().values() {
                let bytes = source_inventory[source["path"].as_str().unwrap()];
                assert!(source["end"].as_u64().unwrap() <= bytes);
            }
        }
        assert_eq!(
            machine["evidence"]["protocol"],
            UHURA_EVIDENCE_SUMMARY_PROTOCOL
        );
        assert_eq!(machine["evidence"]["artifacts"]["examples"], 12);
        assert!(machine["evidence"].get("failures").is_none());
        assert!(machine["evidence"].get("snapshots").is_none());

        let inspection: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/play/inspect.json",
            })))
            .unwrap();
        assert_eq!(inspection["protocol"], "uhura-inspection/1");
        assert_eq!(
            inspection["interactionGraph"], machine["interactionGraph"],
            "Editor and Play inspection must publish the same semantic graph"
        );
        assert_eq!(
            inspection["graphSources"], machine["graphSources"],
            "Editor and Play inspection must publish the same provenance sidecar"
        );
        assert_eq!(
            to_canonical_json(&inspection["interactionGraph"]),
            to_canonical_json(&machine["interactionGraph"]),
            "the shared semantic graph must be byte-identical after canonical encoding"
        );
        assert_eq!(
            to_canonical_json(&inspection["graphSources"]),
            to_canonical_json(&machine["graphSources"]),
            "the shared source sidecar must be byte-identical after canonical encoding"
        );
        let graph_node_ids = graph_nodes
            .iter()
            .map(|node| node["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        let graph_edge_values = graph_edges
            .iter()
            .map(to_canonical_json)
            .collect::<BTreeSet<_>>();
        let graph_source_nodes = machine["graphSources"]["nodes"].as_array().unwrap();
        let graph_source_edges = machine["graphSources"]["edges"].as_array().unwrap();
        assert_eq!(graph_source_nodes.len(), graph_nodes.len());
        assert_eq!(graph_source_edges.len(), graph_edges.len());
        for entry in graph_source_nodes {
            assert!(graph_node_ids.contains(entry["node"].as_str().unwrap()));
            for source in entry["sources"].as_array().unwrap() {
                let bytes = source_inventory[source["path"].as_str().unwrap()];
                assert!(source["end"].as_u64().unwrap() <= bytes);
            }
        }
        for entry in graph_source_edges {
            assert!(graph_edge_values.contains(&to_canonical_json(&entry["edge"])));
            for source in entry["sources"].as_array().unwrap() {
                let bytes = source_inventory[source["path"].as_str().unwrap()];
                assert!(source["end"].as_u64().unwrap() <= bytes);
            }
        }
        assert_eq!(
            inspection["evidence"]["passed"], true,
            "evidence failures: {:#}",
            inspection["evidence"]["failureCount"]
        );
        assert_eq!(
            inspection["evidence"]["protocol"],
            "uhura-evidence-summary/0"
        );
        assert_eq!(inspection["evidence"]["failureCount"], 0);
        assert_eq!(inspection["evidence"]["artifacts"]["examples"], 12);
        assert!(inspection["evidence"].get("snapshots").is_none());
    }

    #[test]
    fn uhura_evidence_diagnostics_publish_semantic_and_physical_sources() {
        use uhura_core::{EvidenceArtifacts, EvidenceFailure, EvidenceFailureCode, EvidenceReport};

        let sources = vec![
            ("machine.uhura".to_string(), "machine".to_string()),
            (
                "nested/conformance.uhura".to_string(),
                "0123456789abcdefghijklmnop".to_string(),
            ),
        ];
        let mut source_map = SourceMap::new();
        for (path, source) in &sources {
            source_map.add(path.clone(), source.clone());
        }
        let report = EvidenceReport {
            protocol: "uhura-evidence-report/0".into(),
            passed: false,
            scenarios: Vec::new(),
            artifacts: EvidenceArtifacts::default(),
            failures: vec![EvidenceFailure {
                code: EvidenceFailureCode::MissingSnapshot,
                scenario: None,
                step_index: None,
                source_id: "module#declarations[3]".into(),
                source: SourceRef {
                    id: "module#declarations[3]".into(),
                    path: "nested/conformance.uhura".into(),
                    start: 7,
                    end: 19,
                },
                message: "missing pin".into(),
            }],
        };
        let diagnostics = evidence_failure(&report, &source_map, &sources);
        let diagnostic = &diagnostics["diagnostics"][0];
        assert_eq!(diagnostic["file"], "nested/conformance.uhura");
        assert_eq!(diagnostic["span"]["offset"], 7);
        assert_eq!(diagnostic["span"]["len"], 12);
        assert_eq!(diagnostic["sourceId"], "module#declarations[3]");
        assert_eq!(diagnostic["source"]["start"], 7);
        assert_eq!(diagnostic["source"]["end"], 19);
    }

    #[test]
    fn uhura_a0_live_deployment_does_not_depend_on_evidence_source() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-a0-without-evidence-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = tool_root().join("examples/applications/a0-return-desk/answers/uhura-0.4");
        for name in ["machine.uhura", "ui.uhura", "host.toml", "provider.mjs"] {
            fs::copy(source.join(name), root.join(name)).unwrap();
        }
        fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "app.returndesk"
version = 1
language = "0.4"

[modules]
return_desk = "machine.uhura"
ui = "ui.uhura"
"#,
        )
        .unwrap();

        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        let summary = candidate.summary();
        assert!(
            summary.editor_current && summary.play_ok,
            "Uhura 0.4 A0 without evidence diagnostics: {:?}",
            candidate.diagnostics()
        );
        assert_eq!(summary.preview_count, Some(0));
        assert_eq!(summary.replay_derived_count, Some(0));

        let (host, _) = Host::new(test_web_assets(), candidate).unwrap();
        let editor: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/editor/state",
            })))
            .unwrap();
        assert_eq!(editor["protocol"], "uhura-editor-state/5");
        assert_eq!(editor["render"]["previews"], serde_json::json!([]));
        assert_eq!(
            editor["render"]["machine"]["sources"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editor_sse_event_has_only_protocol_and_source_revision() {
        let frame = editor_sse_payload(7);
        let json = frame
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("data: "))
            .expect("data line");
        let event: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            event,
            serde_json::json!({
                "protocol": "uhura-editor-event/0",
                "sourceRevision": 7,
            })
        );
    }

    #[test]
    fn editor_and_play_routes_serve_byte_identical_application_entry() {
        let web = WebAssets {
            files: Arc::new(BTreeMap::new()),
            index: Arc::new(b"<!doctype html><main>Uhura</main>".to_vec()),
            wasm_files: Arc::new(BTreeMap::new()),
        };
        let (_, editor, _) = app_document(&web).unwrap();
        let (_, play, _) = app_document(&web).unwrap();
        assert_eq!(editor, play);
        assert_eq!(editor.as_slice(), b"<!doctype html><main>Uhura</main>");
    }

    #[test]
    fn api_routes_are_explicit_and_play_is_fully_namespaced() {
        assert_eq!(api_route("/api/editor/state"), Some(ApiRoute::EditorState));
        assert_eq!(
            api_route("/api/editor/events"),
            Some(ApiRoute::EditorEvents)
        );
        assert_eq!(
            api_route("/api/editor/icon-fonts.json"),
            Some(ApiRoute::EditorIconFonts)
        );
        assert_eq!(
            api_route("/api/editor/icon-fonts/0123.woff2"),
            Some(ApiRoute::EditorIconFont("0123.woff2"))
        );
        assert_eq!(api_route("/api/play/events"), Some(ApiRoute::PlayEvents));
        assert_eq!(
            api_route("/api/play/icon-fonts.json"),
            Some(ApiRoute::PlayIconFonts)
        );
        assert_eq!(
            api_route("/api/play/icon-fonts/0123.woff2"),
            Some(ApiRoute::PlayIconFont("0123.woff2"))
        );
        assert_eq!(
            api_route("/api/play/ir.json"),
            Some(ApiRoute::PlayArtifact(PlayArtifact::Ir))
        );
        assert_eq!(
            api_route("/api/play/inspect.json"),
            Some(ApiRoute::PlayArtifact(PlayArtifact::Inspect))
        );
        assert_eq!(
            api_route("/api/play/assets/avatar.jpg"),
            Some(ApiRoute::PlayAsset("avatar.jpg"))
        );
        assert_eq!(
            api_route("/api/play/wasm/uhura_wasm.js"),
            Some(ApiRoute::PlayWasm("uhura_wasm.js"))
        );
        assert_eq!(api_route("/ir.json"), None);
        assert_eq!(api_route("/events"), None);
        assert_eq!(api_route("/api/play/icons.json"), Some(ApiRoute::Unknown));
        assert_eq!(api_route("/api/nope"), Some(ApiRoute::Unknown));
    }

    #[test]
    fn editor_and_play_serve_scoped_icon_font_manifests_and_exact_bytes() {
        let font = b"checked-foundation-woff2";
        let hash = sha256_hex(font);
        let host = host_with_icon_fonts(font);

        for (scope, version_name) in [("editor", "revision"), ("play", "generation")] {
            let manifest_url = format!("/api/{scope}/icon-fonts.json");
            let manifest_response = host.route(RouteRequest {
                method: RequestMethod::Get,
                url: &manifest_url,
            });
            assert_eq!(manifest_response.status, 200, "GET {manifest_url}");
            assert!(manifest_response.headers.iter().any(|(name, value)| {
                name == "Content-Type" && value == "application/json; charset=utf-8"
            }));
            if scope == "play" {
                assert!(
                    manifest_response
                        .headers
                        .iter()
                        .any(|(name, value)| { name == "X-Uhura-Generation" && value == "1" })
                );
            }
            let manifest: serde_json::Value =
                serde_json::from_slice(&response_bytes(manifest_response)).unwrap();
            assert_eq!(manifest["protocol"], "uhura-icon-fonts/0");
            assert_eq!(manifest[version_name], 1);
            assert_eq!(manifest["default"], "foundation");
            assert_eq!(
                manifest["families"]["foundation"]["font"],
                format!("/api/{scope}/icon-fonts/{hash}.woff2")
            );
            assert_eq!(manifest["families"]["foundation"]["sha256"], hash);
            assert_eq!(
                manifest["families"]["foundation"]["glyphs"],
                serde_json::json!({ "heart": 0xe001, "home": 0xe000 })
            );

            let font_url = format!("/api/{scope}/icon-fonts/{hash}.woff2");
            let font_response = host.route(RouteRequest {
                method: RequestMethod::Get,
                url: &font_url,
            });
            assert_eq!(font_response.status, 200, "GET {font_url}");
            assert!(
                font_response
                    .headers
                    .iter()
                    .any(|(name, value)| { name == "Content-Type" && value == "font/woff2" })
            );
            assert_eq!(response_bytes(font_response), font);

            let head_response = host.route(RouteRequest {
                method: RequestMethod::Head,
                url: &font_url,
            });
            assert_eq!(head_response.status, 200, "HEAD {font_url}");
            assert!(head_response.headers.iter().any(|(name, value)| {
                name == "Content-Length" && value == &font.len().to_string()
            }));
            assert!(response_bytes(head_response).is_empty());
        }
    }

    #[test]
    fn project_resource_manifest_loads_checked_assets_and_icons_into_editor_and_play() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-icon-resources-{}-{unique}",
            std::process::id()
        ));
        let source = tool_root().join("examples/applications/a0-return-desk/answers/uhura-0.4");
        fs::create_dir_all(root.join("icons/brand")).unwrap();
        fs::create_dir_all(root.join("assets")).unwrap();
        for name in ["machine.uhura", "ui.uhura", "host.toml", "provider.mjs"] {
            fs::copy(source.join(name), root.join(name)).unwrap();
        }
        let resources = tool_root().join("resources/icon-fonts/lucide");
        let font = fs::read(resources.join("lucide.woff2")).unwrap();
        fs::write(root.join("icons/brand/icons.woff2"), &font).unwrap();
        fs::copy(
            resources.join("glyphs.json"),
            root.join("icons/brand/glyphs.json"),
        )
        .unwrap();
        let asset = b"checked webp";
        let asset_hash = sha256_hex(asset);
        fs::write(root.join("assets/hero.webp"), asset).unwrap();
        fs::write(
            root.join("assets/manifest.toml"),
            format!(
                r#"[assets.hero]
file = "hero.webp"
alt = "Checked hero"
sha256 = "{asset_hash}"
"#
            ),
        )
        .unwrap();
        fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "app.returndesk"
version = 1
language = "0.4"

[modules]
return_desk = "machine.uhura"
ui = "ui.uhura"

[assets]
manifest = "assets/manifest.toml"

[icons]
default = "brand"

[icons.brand]
font = "icons/brand/icons.woff2"
glyphs = "icons/brand/glyphs.json"
"#,
        )
        .unwrap();

        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        assert!(
            candidate.summary().editor_current && candidate.summary().play_ok,
            "{:?}",
            candidate.diagnostics()
        );
        let (host, _) = Host::new(test_web_assets(), candidate).unwrap();
        let editor: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/editor/state",
            })))
            .unwrap();
        assert_eq!(editor["render"]["assets"]["hero"]["alt"], "Checked hero");
        assert_eq!(
            editor["render"]["assets"]["hero"]["dataUri"],
            format!("data:image/webp;base64,{}", super::base64(asset))
        );
        let play_asset = host.route(RouteRequest {
            method: RequestMethod::Get,
            url: "/api/play/assets/hero.webp",
        });
        assert_eq!(play_asset.status, 200);
        assert_eq!(response_bytes(play_asset), asset);

        let hash = sha256_hex(&font);
        for scope in ["editor", "play"] {
            let manifest: serde_json::Value =
                serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                    method: RequestMethod::Get,
                    url: &format!("/api/{scope}/icon-fonts.json"),
                })))
                .unwrap();
            assert_eq!(manifest["default"], "brand");
            assert!(manifest["families"]["lucide"].is_object());
            assert_eq!(manifest["families"]["brand"]["sha256"], hash);
            assert_eq!(
                manifest["families"]["brand"]["font"],
                format!("/api/{scope}/icon-fonts/{hash}.woff2")
            );
            assert_eq!(manifest["families"]["brand"]["glyphs"]["home"], 57589);
        }

        fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "app.returndesk"
version = 1
language = "0.4"

[modules]
return_desk = "machine.uhura"
ui = "ui.uhura"

[assets]
manifest = "assets/manifest.toml"

[icons]
default = "brand"

[icons.brand]
font = "icons/brand/icons.woff2"
glyphs = "icons/brand/missing.json"
"#,
        )
        .unwrap();
        let rejected = super::build_candidate(&crate::source::capture_project_snapshot(&root), 2);
        assert!(!rejected.summary().editor_current);
        assert!(!rejected.summary().play_ok);
        assert_eq!(
            rejected.diagnostics().editor["diagnostics"][0]["code"],
            "UH2010"
        );
        assert_eq!(
            rejected.diagnostics().play["diagnostics"][0]["code"],
            "UH2010"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn icon_font_routes_reject_malformed_or_unknown_digests() {
        let host = host_with_icon_fonts(b"checked-font");
        for file in [
            "font.woff2",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.woff2",
            "../../secret.woff2",
            "0123.ttf",
        ] {
            let url = format!("/api/editor/icon-fonts/{file}");
            assert_eq!(
                host.route(RouteRequest {
                    method: RequestMethod::Get,
                    url: &url,
                })
                .status,
                400,
                "GET {url}",
            );
        }

        let unknown = "0".repeat(64);
        let url = format!("/api/play/icon-fonts/{unknown}.woff2");
        assert_eq!(
            host.route(RouteRequest {
                method: RequestMethod::Get,
                url: &url,
            })
            .status,
            404,
        );
    }

    #[test]
    fn rejected_publication_retains_matching_editor_and_play_icon_resources() {
        let font = b"last-good-font";
        let hash = sha256_hex(font);
        let host = host_with_icon_fonts(font);

        let report = host
            .publish(ClientCandidate {
                revision: 2,
                source_fingerprint: ProjectSourceFingerprint::default(),
                source_revision_id: "test-source-revision-2".into(),
                editor: Err(editor_rejection("broken Editor")),
                play: Err(diagnostics("broken Play")),
                checked_routes: None,
            })
            .unwrap();
        assert_eq!(report.source_revision, 2);
        assert_eq!(report.play_generation, 2);
        assert!(!report.editor_current);
        assert!(!report.play_ok);

        let editor_manifest: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/editor/icon-fonts.json",
            })))
            .unwrap();
        let play_manifest: serde_json::Value =
            serde_json::from_slice(&response_bytes(host.route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/play/icon-fonts.json",
            })))
            .unwrap();
        assert_eq!(editor_manifest["revision"], 1);
        assert_eq!(editor_manifest.get("generation"), None);
        assert_eq!(play_manifest["generation"], 2);
        assert_eq!(play_manifest.get("revision"), None);
        assert_eq!(editor_manifest["families"]["foundation"]["sha256"], hash);
        assert_eq!(play_manifest["families"]["foundation"]["sha256"], hash);

        for scope in ["editor", "play"] {
            let url = format!("/api/{scope}/icon-fonts/{hash}.woff2");
            let response = host.route(RouteRequest {
                method: RequestMethod::Get,
                url: &url,
            });
            assert_eq!(response.status, 200);
            if scope == "play" {
                assert!(
                    response
                        .headers
                        .iter()
                        .any(|(name, value)| { name == "X-Uhura-Generation" && value == "2" })
                );
            }
            assert_eq!(response_bytes(response), font);
        }
    }

    #[test]
    fn head_strips_every_byte_response_without_changing_get_metadata() {
        let host = test_host(test_web_assets());
        for (url, expected_status) in [
            ("/api/editor/state", 200),
            ("/api/nope", 404),
            ("/api/play/ir.json", 503),
        ] {
            let get = host.route(RouteRequest {
                method: RequestMethod::Get,
                url,
            });
            let head = host.route(RouteRequest {
                method: RequestMethod::Head,
                url,
            });

            assert_eq!(get.status, expected_status, "GET {url}");
            assert_eq!(head.status, get.status, "HEAD {url}");
            assert_eq!(head.headers, get.headers, "HEAD {url}");
            let content_length = get
                .headers
                .iter()
                .find_map(|(name, value)| {
                    (name == "Content-Length").then(|| value.parse::<usize>().unwrap())
                })
                .expect("byte responses report their GET representation length");
            assert_eq!(response_bytes(get).len(), content_length, "GET {url}");
            assert!(content_length > 0, "GET {url}");
            assert!(response_bytes(head).is_empty(), "HEAD {url}");
        }

        for url in ["/api/editor/events", "/api/play/events"] {
            let response = host.route(RouteRequest {
                method: RequestMethod::Head,
                url,
            });
            assert_eq!(response.status, 405, "HEAD {url}");
            assert!(
                response
                    .headers
                    .iter()
                    .any(|(name, value)| name == "Allow" && value == "GET"),
                "HEAD {url}",
            );
            assert!(response_bytes(response).is_empty(), "HEAD {url}");
        }
    }

    #[test]
    fn play_inspection_artifact_is_coherent_with_checked_ir_and_spans() {
        let root = tool_root().join("examples/instagram/client");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        let good = candidate.play.expect("canonical example checks");
        let inspection: serde_json::Value =
            serde_json::from_str(&good.inspect_json).expect("inspection JSON");
        let program = Program::from_json(&good.ir).expect("served IR loads");
        let machine = inspection["machine"].as_str().expect("inspected machine");

        assert_eq!(good.inspect_json, to_canonical_json(&inspection));
        assert_eq!(inspection["protocol"], "uhura-inspection/1");
        assert_eq!(
            inspection["machineProgramHash"],
            program.machine_program.program_hashes[machine]
        );
        assert!(
            inspection["interactionGraph"]["nodes"]
                .as_array()
                .is_some_and(|nodes| !nodes.is_empty()),
            "checked machine topology is published",
        );
        let graph_nodes = inspection["interactionGraph"]["nodes"].as_array().unwrap();
        let node_kinds = graph_nodes
            .iter()
            .filter_map(|node| node["kind"].as_str())
            .collect::<BTreeSet<_>>();
        assert!(
            [
                "module",
                "part",
                "computed",
                "invariant",
                "update",
                "observation",
            ]
            .into_iter()
            .all(|kind| node_kinds.contains(kind)),
            "authored facts erased from runtime IR must survive inspection",
        );
        assert_eq!(
            graph_nodes
                .iter()
                .filter(|node| node["kind"] == "part")
                .filter_map(|node| node["label"].as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["notice", "notice_controls"]),
        );
        let graph_edges = inspection["interactionGraph"]["edges"].as_array().unwrap();
        let edge_kinds = graph_edges
            .iter()
            .filter_map(|edge| edge["kind"].as_str())
            .collect::<BTreeSet<_>>();
        assert!(
            ["composes", "reads", "calls", "observes"]
                .into_iter()
                .all(|kind| edge_kinds.contains(kind)),
            "composition, reads, update calls, and committed observations are inspectable",
        );
        let node_kinds_by_id = graph_nodes
            .iter()
            .filter_map(|node| Some((node["id"].as_str()?, node["kind"].as_str()?)))
            .collect::<BTreeMap<_, _>>();
        assert!(
            graph_edges.iter().any(|edge| {
                edge["kind"] == "reads"
                    && edge["from"]
                        .as_str()
                        .and_then(|node| node_kinds_by_id.get(node))
                        .is_some_and(|kind| matches!(*kind, "input" | "update"))
            }),
            "an Instagram handler or update publishes its direct authored reads",
        );
        let outcome_policies = inspection["interactionGraph"]["outcome_policies"]
            .as_object()
            .expect("canonical outcome policy table");
        let actual_outcomes = graph_nodes
            .iter()
            .filter(|node| node["kind"] == "outcome")
            .map(|node| {
                let id = node["id"].as_str().expect("outcome ID");
                (
                    node["label"].as_str().expect("outcome label"),
                    outcome_policies[id]
                        .as_str()
                        .expect("outcome commit or abort policy"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            actual_outcomes,
            BTreeMap::from([
                ("Accepted", "commit"),
                ("Blocked", "abort"),
                ("Duplicate", "abort"),
                ("Invalid", "abort"),
                ("Stale", "abort"),
            ]),
            "Editor and Play inspect the exact policy of every Instagram outcome",
        );
        let graph_source_paths = inspection_graph_source_paths(&inspection);
        assert_eq!(
            graph_source_paths,
            BTreeSet::from([
                "machine.uhura".to_string(),
                "parts.uhura".to_string(),
                "ui.uhura".to_string(),
            ]),
            "every Instagram graph source resolves to its admitted authored file",
        );
        assert!(
            inspection["graphSources"]
                .as_object()
                .is_some_and(|sources| !sources.is_empty()),
            "semantic topology retains physical source navigation",
        );
        assert_eq!(
            inspection["graphSources"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            graph_nodes.len(),
        );
        assert!(
            inspection["graphSources"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .chain(
                    inspection["graphSources"]["edges"]
                        .as_array()
                        .unwrap()
                        .iter(),
                )
                .all(|entry| entry["sources"]
                    .as_array()
                    .is_some_and(|sources| !sources.is_empty())),
            "every authored graph fact retains valid physical provenance",
        );
    }

    #[test]
    fn play_asset_paths_decode_spaces_and_safe_nested_slashes_once() {
        assert_eq!(
            decode_play_asset_path("gallery%2Fsummer%20day.jpg").unwrap(),
            "gallery/summer day.jpg"
        );
        assert_eq!(
            decode_play_asset_path("%252e%252e%2Fsecret.jpg").unwrap(),
            "%2e%2e/secret.jpg"
        );
    }

    #[test]
    fn play_asset_paths_reject_unsafe_or_malformed_input() {
        for encoded in [
            "",
            "/absolute.jpg",
            "%2Fabsolute.jpg",
            "album//photo.jpg",
            "album%2F%2Fphoto.jpg",
            ".",
            "%2e",
            "..",
            "%2e%2e%2Fsecret.jpg",
            "album/./photo.jpg",
            "album%2F%2e%2e%2Fsecret.jpg",
            "album\\photo.jpg",
            "album%5Cphoto.jpg",
            "%",
            "%2",
            "%GG",
            "%FF.jpg",
            "%00.jpg",
        ] {
            let error = decode_play_asset_path(encoded).unwrap_err();
            assert_eq!(error.0, 400, "{encoded}");
        }
    }

    #[test]
    fn play_asset_serving_preserves_decoded_extension_and_never_decodes_twice() {
        let assets = BTreeMap::from([(
            "summer album/clip one.mp4".to_string(),
            Arc::<[u8]>::from(&b"fixture-video"[..]),
        )]);

        let (kind, bytes, generation) =
            captured_play_asset(&assets, "summer%20album%2Fclip%20one.mp4").unwrap();
        assert_eq!(kind, "video/mp4");
        assert_eq!(bytes, b"fixture-video");
        assert_eq!(generation, None);

        let error = captured_play_asset(&assets, "%252e%252e%2Fsecret.jpg").unwrap_err();
        assert_eq!(error.0, 404);
    }

    #[test]
    fn application_and_wasm_assets_decode_once_and_reject_unsafe_paths() {
        let files = BTreeMap::from([
            (
                "assets/summer day.js".to_string(),
                WebFile {
                    bytes: Arc::new(b"application".to_vec()),
                    content_type: content_type("js"),
                },
            ),
            (
                "assets/%2e%2e/literal.js".to_string(),
                WebFile {
                    bytes: Arc::new(b"literal application percent escapes".to_vec()),
                    content_type: content_type("js"),
                },
            ),
        ]);
        let wasm_files = BTreeMap::from([
            (
                "runtime glue.js".to_string(),
                WebFile {
                    bytes: Arc::new(b"wasm glue".to_vec()),
                    content_type: content_type("js"),
                },
            ),
            (
                "%2e%2e/literal.wasm".to_string(),
                WebFile {
                    bytes: Arc::new(b"literal wasm percent escapes".to_vec()),
                    content_type: content_type("wasm"),
                },
            ),
        ]);
        let host = test_host(WebAssets {
            files: Arc::new(files),
            index: Arc::new(b"<!doctype html><main>Uhura</main>".to_vec()),
            wasm_files: Arc::new(wasm_files),
        });

        for (url, expected) in [
            ("/assets/summer%20day.js", b"application".as_slice()),
            ("/assets%2Fsummer%20day.js", b"application".as_slice()),
            ("/api/play/wasm/runtime%20glue.js", b"wasm glue".as_slice()),
            (
                "/assets/%252e%252e/literal.js",
                b"literal application percent escapes".as_slice(),
            ),
            (
                "/api/play/wasm/%252e%252e%2Fliteral.wasm",
                b"literal wasm percent escapes".as_slice(),
            ),
        ] {
            let response = host.route(RouteRequest {
                method: RequestMethod::Get,
                url,
            });
            assert_eq!(response.status, 200, "GET {url}");
            assert_eq!(response_bytes(response), expected, "GET {url}");
        }

        for url in [
            "/assets/%2e%2e/secret.js",
            "/assets%2F%2e%2e%2Fsecret.js",
            "/assets/%5Csecret.js",
            "/assets/%GG.js",
            "/api/play/wasm/%2e%2e/secret.wasm",
            "/api/play/wasm/%5Csecret.wasm",
            "/api/play/wasm/%GG.wasm",
        ] {
            let response = host.route(RouteRequest {
                method: RequestMethod::Get,
                url,
            });
            assert_eq!(response.status, 400, "GET {url}");
        }
    }

    fn test_web_assets() -> WebAssets {
        WebAssets {
            files: Arc::new(BTreeMap::new()),
            index: Arc::new(b"<!doctype html><main>Uhura</main>".to_vec()),
            wasm_files: Arc::new(BTreeMap::new()),
        }
    }

    fn next_event(stream: &EventStream) -> serde_json::Value {
        let EventStreamPoll::Frame(frame) = stream.next_frame_timeout(Duration::from_secs(1))
        else {
            panic!("expected event frame");
        };
        let json = frame
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("data: "))
            .expect("event data line");
        serde_json::from_str(json).expect("event JSON")
    }

    #[test]
    fn stalled_event_subscriber_keeps_only_one_invalidation() {
        let clients = Arc::new(Mutex::new(ClientRegistry::default()));
        let stream = subscribe(&clients, || editor_sse_payload(1)).expect("event stream admission");

        for revision in 2..=128 {
            broadcast(&clients, &editor_sse_payload(revision));
        }

        // The initial frame still invalidates the client's view, so it can
        // refetch the current artifact. Redundant frames did not accumulate.
        assert_eq!(next_event(&stream)["sourceRevision"], 1);
        assert_eq!(
            stream.next_frame_timeout(Duration::from_millis(1)),
            EventStreamPoll::Timeout
        );

        // Draining the slot lets the next publication wake the same stream.
        broadcast(&clients, &editor_sse_payload(129));
        assert_eq!(next_event(&stream)["sourceRevision"], 129);
    }

    #[test]
    fn blocking_event_stream_keepalive_crosses_the_http_chunk_boundary() {
        let (_sender, receiver) = sync_channel(1);
        let mut stream = EventStream {
            receiver,
            buffer: Vec::new(),
            offset: 0,
            subscription_id: 0,
            clients: Weak::new(),
            blocking_keepalive_interval: Duration::ZERO,
            _admission_permit: None,
        };

        let mut output = vec![0; BLOCKING_EVENT_STREAM_WRITE_BOUNDARY + 16];
        let count = stream.read(&mut output).expect("keepalive read");
        assert_eq!(count, BLOCKING_EVENT_STREAM_WRITE_BOUNDARY + 3);
        assert_eq!(output[0], b':');
        assert!(
            output[1..BLOCKING_EVENT_STREAM_WRITE_BOUNDARY + 1]
                .iter()
                .all(|byte| *byte == b' ')
        );
        assert_eq!(
            &output[BLOCKING_EVENT_STREAM_WRITE_BOUNDARY + 1..count],
            b"\n\n"
        );
    }

    #[test]
    fn framed_event_stream_poll_does_not_synthesize_keepalives() {
        let clients = Arc::new(Mutex::new(ClientRegistry::default()));
        let stream =
            subscribe_with_blocking_keepalive(&clients, || editor_sse_payload(1), Duration::ZERO)
                .expect("event stream admission");

        assert_eq!(next_event(&stream)["sourceRevision"], 1);
        assert_eq!(stream.try_next_frame(), EventStreamPoll::Timeout);
    }

    #[test]
    fn dropping_event_stream_unregisters_immediately() {
        let clients = Arc::new(Mutex::new(ClientRegistry::default()));
        let stream = subscribe(&clients, || editor_sse_payload(1)).expect("event stream admission");
        assert_eq!(clients.lock().expect("clients lock").clients.len(), 1);

        drop(stream);

        assert!(clients.lock().expect("clients lock").clients.is_empty());
    }

    #[test]
    fn host_event_admission_is_shared_bounded_and_reusable() {
        let host = test_host(test_web_assets());
        let admission = Arc::clone(&host.editor_clients.lock().expect("clients lock").admission);
        let active_subscribers = || {
            host.editor_clients
                .lock()
                .expect("clients lock")
                .clients
                .len()
                + host
                    .play_clients
                    .lock()
                    .expect("clients lock")
                    .clients
                    .len()
        };
        let open_stream = |url| {
            let response = host.route(RouteRequest {
                method: RequestMethod::Get,
                url,
            });
            assert_eq!(response.status, 200, "GET {url}");
            match response.body {
                RouteBody::Events(stream) => stream,
                RouteBody::Bytes(_) => panic!("GET {url} should return an event stream"),
            }
        };

        let mut streams = (0..MAX_EVENT_STREAMS_PER_HOST)
            .map(|index| {
                open_stream(if index % 2 == 0 {
                    "/api/editor/events"
                } else {
                    "/api/play/events"
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(active_subscribers(), MAX_EVENT_STREAMS_PER_HOST);
        assert_eq!(
            *admission.active.lock().expect("event admission lock"),
            MAX_EVENT_STREAMS_PER_HOST
        );

        let saturated = host.route(RouteRequest {
            method: RequestMethod::Get,
            url: "/api/editor/events?retry=1",
        });
        assert_eq!(saturated.status, 503);
        assert!(
            saturated
                .headers
                .iter()
                .any(|(name, value)| { name == "Retry-After" && value == "1" })
        );
        assert!(
            String::from_utf8(response_bytes(saturated))
                .unwrap()
                .contains("too many active Uhura event streams")
        );
        assert_eq!(active_subscribers(), MAX_EVENT_STREAMS_PER_HOST);

        drop(streams.pop().expect("one admitted stream"));
        assert_eq!(active_subscribers(), MAX_EVENT_STREAMS_PER_HOST - 1);
        streams.push(open_stream("/api/play/events"));
        assert_eq!(active_subscribers(), MAX_EVENT_STREAMS_PER_HOST);

        drop(streams);
        assert_eq!(active_subscribers(), 0, "registries remove dropped streams");
        assert_eq!(
            *admission.active.lock().expect("event admission lock"),
            0,
            "dropped streams return all permits"
        );

        let fresh = open_stream("/api/editor/events");
        assert_eq!(active_subscribers(), 1);
        drop(fresh);
        assert_eq!(active_subscribers(), 0);
    }

    #[test]
    fn host_publication_is_coherent_and_keeps_event_streams_stable() {
        let root = tool_root().join("examples/instagram/client");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        let (host, first) = super::Host::new(test_web_assets(), candidate).unwrap();
        assert_eq!(first.source_revision, 1);
        assert_eq!(first.play_generation, 1);
        assert!(first.editor_current);
        assert!(first.play_ok);
        assert_eq!(first.preview_count, Some(91));

        let editor_events = match host
            .route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/editor/events",
            })
            .body
        {
            RouteBody::Events(stream) => stream,
            RouteBody::Bytes(_) => panic!("expected Editor event stream"),
        };
        let play_events = match host
            .route(RouteRequest {
                method: RequestMethod::Get,
                url: "/api/play/events",
            })
            .body
        {
            RouteBody::Events(stream) => stream,
            RouteBody::Bytes(_) => panic!("expected Play event stream"),
        };
        assert_eq!(next_event(&editor_events)["sourceRevision"], 1);
        assert_eq!(next_event(&play_events)["generation"], 1);
        assert_eq!(
            editor_events.next_frame_timeout(Duration::from_millis(1)),
            EventStreamPoll::Timeout
        );

        let second = host.publish(super::build_candidate(&snapshot, 2)).unwrap();
        assert_eq!(second.source_revision, 2);
        assert_eq!(second.play_generation, 2);
        assert!(second.editor_current);
        assert!(second.play_ok);
        assert_eq!(host.source_revision(), 2);
        assert_eq!(next_event(&editor_events)["sourceRevision"], 2);
        assert_eq!(next_event(&play_events)["generation"], 2);

        let mut response = host.route(RouteRequest {
            method: RequestMethod::Get,
            url: "/api/play/ir.json",
        });
        assert_eq!(response.status, 200);
        assert!(
            response
                .headers
                .iter()
                .any(|(name, value)| { name == "X-Uhura-Generation" && value == "2" })
        );
        let mut body = String::new();
        response.body.read_to_string(&mut body).unwrap();
        assert!(!body.is_empty());
    }

    #[test]
    fn candidate_exposes_listenerless_diagnostics_and_source_identity() {
        let root = tool_root().join("examples/instagram/client");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let first = super::build_candidate(&snapshot, 1);
        let second = super::build_candidate(&snapshot, 9);

        assert!(first.summary().editor_current);
        assert!(first.summary().play_ok);
        assert_eq!(first.summary().preview_count, Some(91));
        assert_eq!(
            first.source_fingerprint(),
            snapshot.fingerprint(),
            "the candidate retains the identity of the bytes it consumed",
        );
        assert_eq!(first.source_id(), snapshot.source_revision_id());
        assert_eq!(
            first.source_id(),
            second.source_id(),
            "publication revision is deliberately outside source identity",
        );

        let diagnostics = first.diagnostics();
        assert_eq!(diagnostics.editor, &serde_json::Value::Null);
        assert_eq!(diagnostics.play, &serde_json::Value::Null);

        let editor = accepted_editor_render(&first);
        let previews = editor["previews"].as_array().unwrap();
        assert_eq!(previews.len(), 91);
        assert!(
            previews
                .iter()
                .all(|preview| preview["sourceFile"] == "ui.uhura")
        );
        let target_kinds = editor["authoring"]["targets"]
            .as_array()
            .unwrap()
            .iter()
            .inspect(|target| assert_eq!(target["file"], "ui.uhura"))
            .map(|target| target["owner"]["kind"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            target_kinds,
            BTreeSet::from(["component", "page", "surface"])
        );
        let annotations = editor["authoring"]["entries"].as_array().unwrap();
        assert_eq!(annotations.len(), 1);
        let annotation = &annotations[0];
        assert_eq!(annotation["class"], "annotation");
        assert_eq!(annotation["kind"], "annotation");
        assert_eq!(
            annotation["text"],
            "The complete post-card presentation, shared across every static PostCard example."
        );
        assert_eq!(annotation["order"], 0);
        let annotation_target = editor["authoring"]["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|target| target["id"] == annotation["targetId"])
            .expect("the annotation retains its checked target");
        assert_eq!(annotation_target["class"], "ui-element");
        assert_eq!(annotation_target["file"], "ui.uhura");
        assert_eq!(annotation_target["label"], "<view class=\"post-card\">");
        assert_eq!(annotation_target["owner"]["kind"], "component");
        assert_eq!(
            annotation_target["owner"]["name"],
            "app.instagram@1::PostCard"
        );
        let post_card_previews = previews
            .iter()
            .filter(|preview| preview["identity"]["subject"] == "app.instagram@1::PostCard")
            .collect::<Vec<_>>();
        assert_eq!(post_card_previews.len(), 5);
        assert!(post_card_previews.iter().all(|preview| {
            preview["provenance"]["occurrences"]
                .as_array()
                .unwrap()
                .iter()
                .any(|occurrence| {
                    occurrence["targetId"] == annotation_target["id"]
                        && occurrence["anchors"]
                            .as_array()
                            .is_some_and(|anchors| !anchors.is_empty())
                })
        }));

        let graph = &editor["interactionGraph"];
        assert_eq!(
            graph["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|node| node["kind"] == "page")
                .count(),
            9
        );
        assert_eq!(
            graph["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|node| node["kind"] == "surface")
                .count(),
            1
        );
        assert!(
            graph["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|node| node["label"] != "app.instagram@1::BottomNav")
        );
        let edges = graph["edges"].as_array().unwrap();
        assert!(edges.iter().any(|edge| {
            edge["kind"] == "navigate"
                && edge["from"] == "page:app.instagram@1::FeedPage"
                && edge["to"] == "page:app.instagram@1::PostPage"
                && edge["event"] == "OpenPost"
        }));
        assert!(edges.iter().any(|edge| {
            edge["kind"] == "present"
                && edge["from"] == "page:app.instagram@1::FeedPage"
                && edge["to"] == "surface:app.instagram@1::CommentsSheet"
                && edge["event"] == "OpenComments"
        }));

        let replay = previews
            .iter()
            .flat_map(|preview| preview["replay"].as_array().unwrap())
            .find(|step| !step["dispatch"].is_null())
            .expect("a derived preview carries checked handler dispatch");
        assert_eq!(
            replay["dispatch"]["definition"],
            "app.instagram@1::Instagram"
        );
        assert_eq!(replay["dispatch"]["on"], replay["label"]);
        assert!(replay["dispatch"]["selected"].is_u64());
        assert_eq!(replay["dispatch"]["guards"][0]["result"], "satisfied");

        let (host, _) = Host::new(test_web_assets(), first).unwrap();
        let editor_bytes = response_bytes(host.route(RouteRequest {
            method: RequestMethod::Get,
            url: "/api/editor/state",
        }));
        assert!(
            editor_bytes.len() <= 30 * 1024 * 1024,
            "Instagram Editor state is {} bytes, above the 30 MiB transport cap",
            editor_bytes.len()
        );
        let editor_state: serde_json::Value = serde_json::from_slice(&editor_bytes).unwrap();
        assert_eq!(editor_state["protocol"], "uhura-editor-state/5");
        let machine = &editor_state["render"]["machine"];
        assert_eq!(machine["protocol"], "uhura-machine-inspection/1");
        assert_eq!(
            machine["evidence"]["protocol"],
            UHURA_EVIDENCE_SUMMARY_PROTOCOL
        );
        assert_eq!(
            machine["evidence"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "artifacts",
                "failureCount",
                "passed",
                "protocol",
                "scenarios",
            ])
        );
        for preview in editor_state["render"]["previews"].as_array().unwrap() {
            let evidence = preview["evidence"]
                .as_object()
                .expect("Instagram previews retain evidence identity");
            assert_eq!(
                evidence.keys().map(String::as_str).collect::<BTreeSet<_>>(),
                BTreeSet::from(["pin", "scenario", "sourceId", "sources"])
            );
            for retired in ["observation", "snapshot", "scenarioReceiptLog"] {
                assert!(
                    !evidence.contains_key(retired),
                    "preview evidence must not publish raw {retired}"
                );
            }
        }

        let inspection_bytes = response_bytes(host.route(RouteRequest {
            method: RequestMethod::Get,
            url: "/api/play/inspect.json",
        }));
        assert!(
            inspection_bytes.len() <= 2 * 1024 * 1024,
            "Instagram Play inspection is {} bytes, above the 2 MiB transport cap",
            inspection_bytes.len()
        );
        let play_inspection: serde_json::Value = serde_json::from_slice(&inspection_bytes).unwrap();
        assert_eq!(
            play_inspection["evidence"]["protocol"],
            UHURA_EVIDENCE_SUMMARY_PROTOCOL
        );
    }

    #[test]
    fn rejected_candidate_exposes_both_standard_diagnostics_envelopes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-candidate-diagnostics-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();

        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        let summary = candidate.summary();
        let diagnostics = candidate.diagnostics();

        assert!(!summary.editor_current);
        assert!(!summary.play_ok);
        for envelope in [diagnostics.editor, diagnostics.play] {
            assert_eq!(envelope["format"], "uhura-diagnostics");
            assert_eq!(envelope["version"], 0);
            assert!(envelope["summary"]["errors"].as_u64().unwrap() > 0);
            assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frontend_locator_uses_a_complete_later_candidate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("uhura-web-dist-{}-{unique}", std::process::id()));
        let missing = root.join("missing");
        let ready = root.join("ready");
        fs::create_dir_all(ready.join("assets")).unwrap();
        fs::write(
            ready.join("index.html"),
            r#"<script type="module" src="/assets/app.js"></script>"#,
        )
        .unwrap();
        fs::write(ready.join("assets/app.js"), "application code").unwrap();

        let web = load_web_app_from(&[missing, ready]).unwrap();
        assert_eq!(
            web.index.as_slice(),
            br#"<script type="module" src="/assets/app.js"></script>"#
        );
        assert!(web.files.contains_key("assets/app.js"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frontend_asset_discovery_reads_only_start_tag_attributes() {
        let references = index_asset_references(
            r#"
                <!doctype html>
                <!-- src="/assets/comment.js" href="/assets/comment.css" -->
                <script>
                    const src = "/assets/inline.js";
                    const href = "/assets/inline.css";
                    const markup = '<link href="/assets/string.css">';
                </script>
                <style>.example { content: 'src="/assets/style.js"'; }</style>
                <div
                    data-code='src="/assets/attribute-value.js"'
                    data-href="/assets/data.css"
                ></div>
                <LINK rel=stylesheet HREF = "/assets/app.css?v=1">
                <script type=module SRC='/assets/app.js#entry'></script>
            "#,
        )
        .unwrap();

        assert_eq!(
            references,
            BTreeSet::from(["assets/app.css".to_string(), "assets/app.js".to_string(),])
        );
    }

    #[test]
    fn frontend_bundle_snapshot_survives_a_dist_rebuild() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-web-snapshot-{}-{unique}",
            std::process::id()
        ));
        let assets = root.join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(
            root.join("index.html"),
            r#"<script src="/assets/app-old.js"></script>"#,
        )
        .unwrap();
        fs::write(assets.join("app-old.js"), "old application").unwrap();
        fs::write(assets.join("app-old.css"), "old styles").unwrap();

        let web = load_web_app_from(std::slice::from_ref(&root)).unwrap();

        fs::remove_dir_all(&root).unwrap();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(
            root.join("index.html"),
            r#"<script src="/assets/app-new.js"></script>"#,
        )
        .unwrap();
        fs::write(root.join("assets/app-new.js"), "new application").unwrap();

        let (index_type, index, _) = app_document(&web).unwrap();
        assert_eq!(index_type, "text/html; charset=utf-8");
        assert_eq!(index, br#"<script src="/assets/app-old.js"></script>"#);
        let (script_type, script, _) = application_path(&web, "/assets/app-old.js").unwrap();
        assert_eq!(script_type, "text/javascript; charset=utf-8");
        assert_eq!(script, b"old application");
        let (style_type, style, _) = application_path(&web, "/assets/app-old.css").unwrap();
        assert_eq!(style_type, "text/css; charset=utf-8");
        assert_eq!(style, b"old styles");
        assert_eq!(
            application_path(&web, "/assets/app-new.js").unwrap_err().0,
            404
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wasm_bundle_snapshot_survives_a_dist_rebuild() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-wasm-snapshot-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join("wasm")).unwrap();
        fs::write(
            root.join("index.html"),
            r#"<script src="/assets/app.js"></script>"#,
        )
        .unwrap();
        fs::write(root.join("assets/app.js"), "application").unwrap();
        fs::write(root.join("wasm/uhura_wasm.js"), "old wasm glue").unwrap();

        let web = WebAssets::from_directories(&root, &root.join("wasm")).unwrap();
        fs::remove_dir_all(root.join("wasm")).unwrap();

        let (kind, bytes, _) = serve_file_map(&web.wasm_files, "uhura_wasm.js").unwrap();
        assert_eq!(kind, "text/javascript; charset=utf-8");
        assert_eq!(bytes, b"old wasm glue");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frontend_locator_rejects_an_index_only_bundle() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-web-index-only-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("index.html"), "incomplete application").unwrap();

        let error = load_web_app_from(std::slice::from_ref(&root)).unwrap_err();
        assert!(error.contains("contains only index.html"), "{error}");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frontend_locator_rejects_missing_index_assets() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-web-missing-asset-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(
            root.join("index.html"),
            r#"<link rel="stylesheet" href="/assets/missing.css"><script src="assets/app.js"></script>"#,
        )
        .unwrap();
        fs::write(root.join("assets/app.js"), "application").unwrap();

        let error = load_web_app_from(std::slice::from_ref(&root)).unwrap_err();
        assert!(
            error.contains("missing application asset: /assets/missing.css"),
            "{error}"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn frontend_locator_rejects_unsafe_bundle_entries_and_paths() {
        use std::os::unix::fs::symlink;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("uhura-web-unsafe-{}-{unique}", std::process::id()));
        let linked = root.join("linked");
        fs::create_dir_all(linked.join("assets")).unwrap();
        fs::write(linked.join("index.html"), "application").unwrap();
        fs::write(linked.join("outside.js"), "outside").unwrap();
        symlink("../outside.js", linked.join("assets/app.js")).unwrap();

        let error = load_web_app_from(std::slice::from_ref(&linked)).unwrap_err();
        assert!(error.contains("unsafe non-regular entry"), "{error}");

        let backslash = root.join("backslash");
        fs::create_dir_all(&backslash).unwrap();
        fs::write(backslash.join("index.html"), "application").unwrap();
        fs::write(backslash.join("assets\\app.js"), "application").unwrap();

        let error = load_web_app_from(std::slice::from_ref(&backslash)).unwrap_err();
        assert!(error.contains("unsafe path"), "{error}");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn query_is_transport_metadata_not_a_client_route() {
        assert_eq!(
            split_request_url("/api/play/provider.js?sha256=abc"),
            ("/api/play/provider.js", Some("sha256=abc"))
        );
        assert_eq!(
            split_request_url("/play?post=42"),
            ("/play", Some("post=42"))
        );
    }

    #[test]
    fn media_and_browser_asset_content_types_are_preserved() {
        assert_eq!(content_type("mp4"), "video/mp4");
        assert_eq!(content_type("webp"), "image/webp");
        assert_eq!(content_type("wasm"), "application/wasm");
        assert_eq!(content_type("woff2"), "font/woff2");
    }
}
