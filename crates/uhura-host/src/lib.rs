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
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use uhura_base::{Severity, sha256_hex, to_canonical_json, to_envelope};
use uhura_check::check;
use uhura_check::fixture::load_fixture;
use uhura_core::ir::ProgramIr;
use uhura_editor_model::{EditorRender, EditorState};

pub mod source;

pub use source::{ProjectSourceFingerprint, ProjectSourceSnapshot, capture_project_snapshot};

const EDITOR_EVENT_PROTOCOL: &str = "uhura-editor-event/0";

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
    fixture_json: String,
    script_json: String,
    boot_json: String,
    icons_json: String,
    config_json: String,
    provider_js: Option<String>,
    play_assets: BTreeMap<String, Arc<[u8]>>,
}

type EditorBuildOutcome = Result<source::EditorModelArtifact, serde_json::Value>;

/// A complete off-path result for one coherently captured source revision.
/// Hosts may inspect its summary, then atomically publish it into [`Host`].
pub struct ClientCandidate {
    revision: u64,
    source_fingerprint: ProjectSourceFingerprint,
    editor: EditorBuildOutcome,
    play: Result<GoodBuild, serde_json::Value>,
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

/// Structured diagnostics produced while building one client candidate.
///
/// Each component is either JSON `null` or a complete
/// `uhura-diagnostics/0` envelope. Accepted Editor and Play outcomes may still
/// carry warnings; rejected outcomes carry their error envelope. Use
/// [`ClientCandidate::summary`] to distinguish acceptance from rejection.
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
            preview_count: editor.map(|artifact| artifact.preview_count),
            replay_derived_count: editor.map(|artifact| artifact.replay_derived_count),
            play_ok: self.play.is_ok(),
        }
    }

    /// Inspect the exact Editor and Play diagnostics produced for this
    /// candidate without publishing it.
    #[must_use]
    pub fn diagnostics(&self) -> CandidateDiagnostics<'_> {
        let editor = match &self.editor {
            Ok(artifact) => &artifact.diagnostics,
            Err(diagnostics) => diagnostics,
        };
        let play = match &self.play {
            Ok(artifact) => &artifact.diagnostics,
            Err(diagnostics) => diagnostics,
        };
        CandidateDiagnostics { editor, play }
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

    /// Deterministic digest form of [`Self::source_fingerprint`].
    #[must_use]
    pub fn source_id(&self) -> String {
        self.source_fingerprint.stable_id()
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
    last_renderable: Option<EditorRender>,
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
    last_renderable: Option<&EditorRender>,
) -> Result<(String, Option<EditorRender>), String> {
    let (state, next_renderable) = match outcome {
        Ok(artifact) => {
            let next_renderable = artifact.render.clone();
            let state = EditorState::current(revision, artifact.diagnostics, artifact.render)
                .map_err(|error| error.to_string())?;
            (state, Some(next_renderable))
        }
        Err(diagnostics) => match last_renderable {
            Some(render) => (
                EditorState::stale(revision, diagnostics, render.clone())
                    .map_err(|error| error.to_string())?,
                Some(render.clone()),
            ),
            None => (
                EditorState::cold_invalid(revision, diagnostics)
                    .map_err(|error| error.to_string())?,
                None,
            ),
        },
    };
    let state_json = state
        .to_canonical_string()
        .map_err(|error| error.to_string())?;
    Ok((state_json, next_renderable))
}

/// Build Editor and Play from exactly the bytes in `snapshot`.
pub fn build_candidate(snapshot: &ProjectSourceSnapshot, revision: u64) -> ClientCandidate {
    let editor =
        source::build_captured_snapshot_at(snapshot, revision).map_err(|failure| failure.envelope);
    let play = recheck_play(&snapshot.files);
    ClientCandidate {
        revision,
        source_fingerprint: snapshot.fingerprint.clone(),
        editor,
        play,
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

// ── Play's independent last-good artifacts ─────────────────────────────────

fn recheck_play(files: &source::ProjectSourceFiles) -> Result<GoodBuild, serde_json::Value> {
    let fail = |message: String| {
        serde_json::json!({
            "format": "uhura-diagnostics",
            "version": 0,
            "summary": { "errors": 1, "warnings": 0 },
            "diagnostics": [{
                "code": "UH9000",
                "rule": "play/recheck",
                "severity": "error",
                "message": message,
            }],
        })
    };
    let input = source::assemble_snapshot_input(files).map_err(|failure| failure.envelope)?;
    let output = check(&input);
    if output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(to_envelope(&output.diagnostics, &output.source_map));
    }
    let Some(lowered) = &output.lowered else {
        return Err(fail("the check produced no program".to_string()));
    };
    let diagnostics = uhura_editor_model::diagnostics_json(&output);
    let program = &lowered.program;

    let manifest = &input.manifest;
    let profile = manifest
        .play
        .values()
        .next()
        .ok_or_else(|| fail("uhura.toml declares no [play.*] profile (§3)".to_string()))?;
    let fixture_rel = manifest.fixtures.get(&profile.fixture).ok_or_else(|| {
        fail(format!(
            "play fixture `{}` is not declared",
            profile.fixture
        ))
    })?;
    let read = |relative: &str| -> Result<String, serde_json::Value> {
        let bytes = files
            .resolve(Path::new(relative))
            .map_err(|error| fail(format!("{relative}: {error}")))?
            .ok_or_else(|| fail(format!("{relative}: missing from the captured project")))?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|error| fail(format!("{relative}: source is not UTF-8: {error}")))
    };
    let fixture_text = read(fixture_rel)?;
    let script_text = read(&format!("fixtures/scripts/{}.toml", profile.script))?;
    let fixture = load_fixture(&fixture_text)
        .map_err(|issues| fail(format!("fixture: {}", issues[0].message)))?;
    let script_json = uhura_fixture::toml_to_json(&script_text).map_err(fail)?;
    let fixture_json = fixture_slices_json(&fixture);
    let script_canonical = to_canonical_json(&script_json);
    uhura_fixture::FixtureDriver::new(&fixture_json, &script_canonical)
        .map_err(|error| fail(format!("script `{}`: {error}", profile.script)))?;

    let (config_json, provider_js) = match &profile.provider {
        Some(provider) => {
            let provider_js = read(&provider.module)?;
            let provider_hash = sha256_hex(provider_js.as_bytes());
            let config_json = to_canonical_json(&serde_json::json!({
                "allow_fixture": profile.allow_fixture,
                "provider": {
                    "kind": "module",
                    "module": format!(
                        "/api/play/provider.js?sha256={provider_hash}"
                    ),
                    "config": &provider.config,
                },
            }));
            (config_json, Some(provider_js))
        }
        None => (
            to_canonical_json(&serde_json::json!({
                "allow_fixture": true,
                "provider": { "kind": "fixture" },
            })),
            None,
        ),
    };

    let mut inspection = uhura_core::inspect::program_graph(program);
    inspection["spans"] =
        serde_json::to_value(&lowered.spans).expect("IR spans are always serializable");

    Ok(GoodBuild {
        diagnostics,
        ir: program.to_canonical_string(),
        inspect_json: to_canonical_json(&inspection),
        stylesheet: output.stylesheet.clone(),
        fixture_json,
        script_json: script_canonical,
        boot_json: boot_envelope(program, &fixture).map_err(fail)?,
        icons_json: structured_icons_json(),
        config_json,
        provider_js,
        play_assets: files.subtree(Path::new("fixtures/assets")),
    })
}

/// The boot deliveries shared by the standalone trace harness and Play host.
pub fn boot_updates(
    program: &ProgramIr,
    fixture: &uhura_check::fixture::FixtureData,
) -> Result<Vec<uhura_port::envelope::ProjectionUpdate>, String> {
    let mut updates = Vec::new();
    for (name, decl) in &program.projections {
        if !decl.boot {
            continue;
        }
        let Some(value) = fixture.get("boot", name.as_str()) else {
            return Err(format!(
                "boot projection `{name}` needs a `boot.{name}` fixture slice (§6.1)"
            ));
        };
        updates.push(uhura_port::envelope::ProjectionUpdate {
            port: decl.port.clone(),
            projection: name.clone(),
            key: None,
            revision: 1,
            value: value.clone(),
        });
    }
    Ok(updates)
}

/// Resolve fixture slices into the canonical `FixtureDriver` input.
pub fn fixture_slices_json(fixture: &uhura_check::fixture::FixtureData) -> String {
    let mut root = serde_json::Map::new();
    for (namespace, slices) in &fixture.slices {
        let mut namespace_map = serde_json::Map::new();
        for (name, value) in slices {
            namespace_map.insert(name.clone(), value.clone());
        }
        root.insert(namespace.clone(), serde_json::Value::Object(namespace_map));
    }
    to_canonical_json(&serde_json::Value::Object(root))
}

/// The `/api/play/boot.json` wire form. Acceptance tests reuse this exact
/// producer, so fixture and browser boot cannot drift.
pub fn boot_envelope(
    program: &ProgramIr,
    fixture: &uhura_check::fixture::FixtureData,
) -> Result<String, String> {
    let updates = boot_updates(program, fixture)?;
    Ok(to_canonical_json(&serde_json::json!({
        "updates": updates
            .iter()
            .map(uhura_port::envelope::ProjectionUpdate::to_json)
            .collect::<Vec<_>>(),
    })))
}

fn structured_icons_json() -> String {
    let icons = uhura_editor_model::icons::table()
        .into_iter()
        .map(|(name, icon)| (name, icon.to_json()))
        .collect::<serde_json::Map<_, _>>();
    to_canonical_json(&serde_json::Value::Object(icons))
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
        Ok((
            Self {
                state: RwLock::new(DevState { play, editor }),
                play_clients: Arc::new(Mutex::new(Vec::new())),
                editor_clients: Arc::new(Mutex::new(Vec::new())),
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
type Clients = Arc<Mutex<Vec<SyncSender<String>>>>;

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
    clients.retain(|sender| match sender.try_send(payload.to_string()) {
        Ok(()) | Err(TrySendError::Full(_)) => true,
        Err(TrySendError::Disconnected(_)) => false,
    });
}

pub struct EventStream {
    receiver: Receiver<String>,
    buffer: Vec<u8>,
    offset: usize,
}

/// One bounded wait on a host-session event stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventStreamPoll {
    Frame(String),
    Timeout,
    Closed,
}

impl EventStream {
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

impl Read for EventStream {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset >= self.buffer.len() {
            match self.receiver.recv() {
                Ok(frame) => {
                    self.buffer = frame.into_bytes();
                    self.offset = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let count = (self.buffer.len() - self.offset).min(output.len());
        output[..count].copy_from_slice(&self.buffer[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

fn subscribe(clients: &Clients, hello: impl FnOnce() -> String) -> EventStream {
    let (sender, receiver) = sync_channel::<String>(1);
    {
        // Registration and snapshot share the broadcast lock: an update can
        // be coalesced with the initial invalidation but can never leave the
        // subscriber without an invalidation to refetch current state.
        let mut clients = clients.lock().expect("clients lock");
        sender
            .try_send(hello())
            .expect("new event queue has one available slot");
        clients.push(sender);
    }
    EventStream {
        receiver,
        buffer: Vec::new(),
        offset: 0,
    }
}

// ── one web application and namespaced transport ───────────────────────────

#[derive(Clone, Debug)]
pub struct WebAssets {
    files: Arc<BTreeMap<String, WebFile>>,
    index: Arc<Vec<u8>>,
    wasm_files: Arc<BTreeMap<String, WebFile>>,
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
        if !bytes[cursor].is_ascii_alphabetic() {
            cursor += 1;
            continue;
        }
        let name_start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
        {
            cursor += 1;
        }
        let name = &index[name_start..cursor];
        if !name.eq_ignore_ascii_case("src") && !name.eq_ignore_ascii_case("href") {
            continue;
        }
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            continue;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let Some(first) = bytes.get(cursor).copied() else {
            break;
        };
        let (value_start, value_end) = if matches!(first, b'\'' | b'"') {
            cursor += 1;
            let start = cursor;
            while bytes.get(cursor).is_some_and(|byte| *byte != first) {
                cursor += 1;
            }
            let end = cursor;
            if cursor < bytes.len() {
                cursor += 1;
            }
            (start, end)
        } else {
            let start = cursor;
            while bytes
                .get(cursor)
                .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>')
            {
                cursor += 1;
            }
            (start, cursor)
        };
        let value = &index[value_start..value_end];
        let path_end = value.find(['?', '#']).unwrap_or(value.len());
        let path = &value[..path_end];
        let lowercase = path.to_ascii_lowercase();
        if !lowercase.ends_with(".js")
            && !lowercase.ends_with(".mjs")
            && !lowercase.ends_with(".css")
        {
            continue;
        }
        if path.starts_with("//")
            || path
                .find(':')
                .is_some_and(|colon| path.find('/').is_none_or(|slash| colon < slash))
        {
            continue;
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
    }
    Ok(references)
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
    Fixture,
    Script,
    Boot,
    Icons,
    Config,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ApiRoute<'a> {
    EditorState,
    EditorEvents,
    PlayEvents,
    PlayArtifact(PlayArtifact),
    PlayProvider,
    PlayAsset(&'a str),
    PlayWasm(&'a str),
    Unknown,
}

fn api_route(path: &str) -> Option<ApiRoute<'_>> {
    let route = match path {
        "/api/editor/state" => ApiRoute::EditorState,
        "/api/editor/events" => ApiRoute::EditorEvents,
        "/api/play/events" => ApiRoute::PlayEvents,
        "/api/play/ir.json" => ApiRoute::PlayArtifact(PlayArtifact::Ir),
        "/api/play/inspect.json" => ApiRoute::PlayArtifact(PlayArtifact::Inspect),
        "/api/play/stylesheet.css" => ApiRoute::PlayArtifact(PlayArtifact::Stylesheet),
        "/api/play/fixture.json" => ApiRoute::PlayArtifact(PlayArtifact::Fixture),
        "/api/play/script.json" => ApiRoute::PlayArtifact(PlayArtifact::Script),
        "/api/play/boot.json" => ApiRoute::PlayArtifact(PlayArtifact::Boot),
        "/api/play/icons.json" => ApiRoute::PlayArtifact(PlayArtifact::Icons),
        "/api/play/config.json" => ApiRoute::PlayArtifact(PlayArtifact::Config),
        "/api/play/provider.js" => ApiRoute::PlayProvider,
        _ => {
            if let Some(relative) = path.strip_prefix("/api/play/assets/")
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
                let stream = subscribe(&self.play_clients, || {
                    play_sse_payload(&self.state.read().expect("state lock").play)
                });
                return event_response(stream);
            }
            Some(ApiRoute::EditorEvents) => {
                if request.method != RequestMethod::Get {
                    return event_method_error(path);
                }
                let stream = subscribe(&self.editor_clients, || {
                    let revision = self
                        .state
                        .read()
                        .expect("state lock")
                        .editor
                        .source_revision;
                    editor_sse_payload(revision)
                });
                return event_response(stream);
            }
            _ => {}
        }

        let outcome = match api_route(path) {
            Some(ApiRoute::EditorState) => editor_state_artifact(&self.state),
            Some(ApiRoute::PlayArtifact(artifact_kind)) => {
                play_artifact(&self.state, artifact_kind)
            }
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
        served_response(request.method, outcome)
    }
}

fn served_response(method: RequestMethod, outcome: Served) -> RouteResponse {
    match outcome {
        Ok((content_type, mut bytes, generation)) => {
            if method == RequestMethod::Head {
                bytes.clear();
            }
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
        PlayArtifact::Fixture => ("json", good.fixture_json.as_bytes()),
        PlayArtifact::Script => ("json", good.script_json.as_bytes()),
        PlayArtifact::Boot => ("json", good.boot_json.as_bytes()),
        PlayArtifact::Icons => ("json", good.icons_json.as_bytes()),
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
        return Err((
            404,
            "the Play profile uses the fixture provider".to_string(),
        ));
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
    let Some(relative) = path.strip_prefix('/') else {
        return app_document(web);
    };
    if relative == "assets" {
        return Err((404, "no such application asset".to_string()));
    }
    if web.files.contains_key(relative) {
        return serve_web_file(web, relative);
    }
    if relative.starts_with("assets/") {
        return serve_web_file(web, relative);
    }
    if relative == "favicon.ico" {
        return favicon(web);
    }
    app_document(web)
}

fn serve_web_file(web: &WebAssets, relative: &str) -> Served {
    if relative.contains('\\')
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || Path::new(relative).is_absolute()
    {
        return Err((400, "bad application asset path".to_string()));
    }
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
            return Err((400, "bad asset path: malformed percent escape".to_string()));
        };
        let Some(low) = bytes.get(index + 2).and_then(|byte| hex_value(*byte)) else {
            return Err((400, "bad asset path: malformed percent escape".to_string()));
        };
        decoded.push((high << 4) | low);
        index += 3;
    }

    let decoded = String::from_utf8(decoded)
        .map_err(|_| (400, "bad asset path: decoded path is not UTF-8".to_string()))?;
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
        return Err((400, "bad asset path".to_string()));
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

fn serve_file_map(files: &BTreeMap<String, WebFile>, relative: &str) -> Served {
    if relative
        .split('/')
        .any(|segment| segment == "." || segment == ".." || segment.is_empty())
        || relative.contains('\\')
        || Path::new(relative).is_absolute()
    {
        return Err((400, "bad path".to_string()));
    }
    let file = files
        .get(relative)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Read;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use uhura_base::to_canonical_json;
    use uhura_editor_model::{Application, AuthoringMetadata, EditorRender, RenderFreshness};

    use crate::source::EditorModelArtifact;

    use super::{
        ApiRoute, EditorHostState, EventStream, EventStreamPoll, PlayArtifact, RequestMethod,
        RouteBody, RouteRequest, WebAssets, api_route, app_document, application_path, broadcast,
        captured_play_asset, content_type, decode_play_asset_path, editor_sse_payload,
        load_web_app_from, recheck_play, serve_file_map, split_request_url, subscribe, tool_root,
    };

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
            icons: BTreeMap::new(),
            assets: BTreeMap::new(),
        }
    }

    fn artifact(revision: u64, name: &str) -> EditorModelArtifact {
        EditorModelArtifact {
            render: render(revision, name),
            preview_count: 0,
            replay_derived_count: 0,
            diagnostics: serde_json::Value::Null,
        }
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

    fn state_json(state: &EditorHostState) -> serde_json::Value {
        serde_json::from_str(&state.state_json).expect("state JSON")
    }

    #[test]
    fn editor_transitions_current_to_stale_and_recovers() {
        let mut state = EditorHostState::initial(Ok(artifact(1, "first"))).unwrap();
        let first = state_json(&state);
        assert_eq!(first["sourceRevision"], 1);
        assert_eq!(first["render"]["freshness"], "current");
        assert_eq!(first["render"]["revision"], 1);

        state.apply(2, Err(diagnostics("broken"))).unwrap();
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
        let mut state = EditorHostState::initial(Err(diagnostics("cold"))).unwrap();
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
        assert_eq!(api_route("/api/play/events"), Some(ApiRoute::PlayEvents));
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
        assert_eq!(api_route("/api/nope"), Some(ApiRoute::Unknown));
    }

    #[test]
    fn play_inspection_artifact_is_coherent_with_checked_ir_and_spans() {
        let root = tool_root().join("examples/instagram-uhura");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let good = recheck_play(&snapshot.files).expect("canonical example checks");
        let inspection: serde_json::Value =
            serde_json::from_str(&good.inspect_json).expect("inspection JSON");
        let program = uhura_core::ir::load_program(&good.ir).expect("served IR loads");

        assert_eq!(good.inspect_json, to_canonical_json(&inspection));
        assert_eq!(inspection["protocol"], "uhura-inspect/0");
        assert_eq!(inspection["kind"], "program");
        assert_eq!(inspection["span-offset-encoding"], "utf-8-bytes");
        assert_eq!(inspection["ir"]["hash"], program.hash());
        assert!(
            inspection["nodes"]
                .as_array()
                .expect("graph nodes")
                .iter()
                .any(|node| node["id"] == "pages.feed/handler/0"),
            "handler ids align with trace selection ids",
        );
        assert!(inspection["spans"]["pages.feed/handler/0"].is_object());
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
        let clients = Arc::new(Mutex::new(Vec::new()));
        let stream = subscribe(&clients, || editor_sse_payload(1));

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
    fn publication_prunes_disconnected_event_subscribers() {
        let clients = Arc::new(Mutex::new(Vec::new()));
        let stream = subscribe(&clients, || editor_sse_payload(1));
        assert_eq!(clients.lock().expect("clients lock").len(), 1);

        drop(stream);
        broadcast(&clients, &editor_sse_payload(2));

        assert!(clients.lock().expect("clients lock").is_empty());
    }

    #[test]
    fn host_publication_is_coherent_and_keeps_event_streams_stable() {
        let root = tool_root().join("examples/instagram-uhura");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let candidate = super::build_candidate(&snapshot, 1);
        let (host, first) = super::Host::new(test_web_assets(), candidate).unwrap();
        assert_eq!(first.source_revision, 1);
        assert_eq!(first.play_generation, 1);
        assert!(first.editor_current);
        assert!(first.play_ok);

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
        let root = tool_root().join("examples/instagram-uhura");
        let snapshot = crate::source::capture_project_snapshot(&root);
        let first = super::build_candidate(&snapshot, 1);
        let second = super::build_candidate(&snapshot, 9);

        assert!(first.summary().editor_current);
        assert!(first.summary().play_ok);
        assert_eq!(
            first.source_fingerprint(),
            snapshot.fingerprint(),
            "the candidate retains the identity of the bytes it consumed",
        );
        assert_eq!(first.source_id(), snapshot.fingerprint().stable_id());
        assert_eq!(
            first.source_id(),
            second.source_id(),
            "publication revision is deliberately outside source identity",
        );

        let diagnostics = first.diagnostics();
        assert_eq!(diagnostics.editor, &serde_json::Value::Null);
        assert_eq!(diagnostics.play, &serde_json::Value::Null);
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
