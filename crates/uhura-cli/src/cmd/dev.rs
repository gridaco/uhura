//! One native host for the model-driven Editor and interactive Play routes.
//!
//! Rust owns coherent project capture, checking/evaluation, immutable
//! `EditorState`, last-good Play artifacts, and HTTP/SSE transport. The
//! compiled web application owns every browser document and all presentation.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use uhura_base::{Severity, sha256_hex, to_canonical_json, to_envelope};
use uhura_check::check;
use uhura_check::fixture::load_fixture;
use uhura_core::ir::ProgramIr;
use uhura_editor_model::{EditorRender, EditorState};

use crate::CommonArgs;
use crate::cmd::trace::{boot_updates, fixture_slices_json};

const EDITOR_EVENT_PROTOCOL: &str = "uhura-editor-event/0";

pub fn run(common: &CommonArgs, port: u16) -> ExitCode {
    run_host(common, port, PrimarySurface::Play)
}

pub(crate) fn run_with_editor(common: &CommonArgs, port: u16) -> ExitCode {
    run_host(common, port, PrimarySurface::Editor)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrimarySurface {
    Editor,
    Play,
}

impl PrimarySurface {
    fn command(self) -> &'static str {
        match self {
            PrimarySurface::Editor => "uhura editor",
            PrimarySurface::Play => "uhura play",
        }
    }

    fn route(self) -> &'static str {
        match self {
            PrimarySurface::Editor => "/",
            PrimarySurface::Play => "/play",
        }
    }
}

fn run_host(common: &CommonArgs, port: u16, primary: PrimarySurface) -> ExitCode {
    let command = primary.command();
    let web = match WebApp::locate() {
        Ok(web) => Arc::new(web),
        Err(error) => {
            eprintln!("{command}: {error}");
            return ExitCode::from(2);
        }
    };

    let root = common.root.clone();
    let first_observation = project_fingerprint(&root);
    let (editor_outcome, baseline_snapshot) = build_stable_editor(&root, first_observation, 1);
    let baseline = baseline_snapshot.fingerprint.clone();
    match &editor_outcome {
        Ok(artifact) => println!(
            "{command}: Editor revision 1 — {} previews ({} replay-derived)",
            artifact.preview_count, artifact.replay_derived_count
        ),
        Err(_) => {
            println!("{command}: Editor revision 1 rejected — application starts with diagnostics")
        }
    }
    let editor = match EditorHostState::initial(editor_outcome) {
        Ok(editor) => editor,
        Err(error) => {
            eprintln!("{command}: could not publish initial Editor state: {error}");
            return ExitCode::from(2);
        }
    };
    let state = Arc::new(RwLock::new(DevState {
        play: PlayState::default(),
        editor,
    }));
    let play_clients: Clients = Arc::new(Mutex::new(Vec::new()));
    let editor_clients: Clients = Arc::new(Mutex::new(Vec::new()));

    recheck_play_into(&baseline_snapshot.files, &state, &play_clients);
    {
        let state = state.read().expect("state lock");
        match (&state.play.good, state.play.ok) {
            (Some(_), true) => println!("{command}: Play checked clean"),
            (Some(_), false) => {
                println!("{command}: Play check failing — serving the last good build")
            }
            (None, _) => println!("{command}: Play check failing — no good build yet"),
        }
    }

    // Browser assets are resolved before binding so a source-built CLI fails
    // clearly instead of opening an unusable port.
    let server = match tiny_http::Server::http(("127.0.0.1", port)) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("{command}: could not bind 127.0.0.1:{port}: {error}");
            return ExitCode::from(2);
        }
    };
    println!("{command}: http://127.0.0.1:{port}{}", primary.route());
    println!(
        "{command}: {} http://127.0.0.1:{port}{}",
        if primary == PrimarySurface::Editor {
            "Play"
        } else {
            "Editor"
        },
        if primary == PrimarySurface::Editor {
            "/play"
        } else {
            "/"
        }
    );

    // One observer drives both independent products. Editor publication is a
    // complete-state replacement; Play keeps its own last-good generation.
    {
        let root = root.clone();
        let state = Arc::clone(&state);
        let play_clients = Arc::clone(&play_clients);
        let editor_clients = Arc::clone(&editor_clients);
        std::thread::spawn(move || {
            let mut seen = baseline;
            loop {
                std::thread::sleep(Duration::from_millis(150));
                let observed = project_fingerprint(&root);
                if observed == seen {
                    continue;
                }
                let stable = wait_for_stable_fingerprint(&root, observed);
                if stable == seen {
                    continue;
                }

                let revision = state.read().expect("state lock").editor.source_revision + 1;
                let (outcome, settled) = build_stable_editor(&root, stable, revision);
                seen = settled.fingerprint.clone();
                let report = outcome
                    .as_ref()
                    .ok()
                    .map(|artifact| (artifact.preview_count, artifact.replay_derived_count));
                if let Err(error) =
                    publish_editor_candidate(revision, outcome, &state, &editor_clients)
                {
                    // This is an internal ordering/contract break, not an
                    // author diagnostic. Leave the prior atomic state intact.
                    eprintln!("uhura host: could not publish Editor revision {revision}: {error}");
                } else if let Some((previews, derived)) = report {
                    println!(
                        "uhura host: Editor revision {revision} current — {previews} previews \
                         ({derived} replay-derived)"
                    );
                } else {
                    println!(
                        "uhura host: Editor revision {revision} rejected — last render is stale"
                    );
                }

                recheck_play_into(&settled.files, &state, &play_clients);
                let play = &state.read().expect("state lock").play;
                println!(
                    "uhura host: Play generation {} — {}",
                    play.generation,
                    if play.ok {
                        "ok, clients reload"
                    } else {
                        "check failing, last-good runtime retained"
                    }
                );
            }
        });
    }

    let server = Arc::new(server);
    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        let play_clients = Arc::clone(&play_clients);
        let editor_clients = Arc::clone(&editor_clients);
        let root = root.clone();
        let web = Arc::clone(&web);
        std::thread::spawn(move || {
            handle(request, &root, &web, &state, &play_clients, &editor_clients)
        });
    }
    ExitCode::SUCCESS
}

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
    ir: String,
    inspect_json: String,
    stylesheet: String,
    fixture_json: String,
    script_json: String,
    boot_json: String,
    icons_json: String,
    config_json: String,
    provider_js: Option<String>,
}

type EditorBuildOutcome = Result<super::editor_model::EditorModelArtifact, serde_json::Value>;

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

fn publish_editor_candidate(
    revision: u64,
    outcome: EditorBuildOutcome,
    state: &RwLock<DevState>,
    clients: &Clients,
) -> Result<(), String> {
    {
        let mut state = state.write().expect("state lock");
        state.editor.apply(revision, outcome)?;
    }
    broadcast(clients, &editor_sse_payload(revision));
    Ok(())
}

/// Build until the exact captured bytes equal the observation both before and
/// after evaluation. An edit landing during work can never publish a mixed
/// model or an older candidate over a newer one.
fn build_stable_editor(
    root: &Path,
    mut before: super::editor_model::ProjectSourceFingerprint,
    revision: u64,
) -> (
    EditorBuildOutcome,
    super::editor_model::ProjectSourceSnapshot,
) {
    loop {
        let snapshot = super::editor_model::capture_project_snapshot(root);
        if before != snapshot.fingerprint {
            before = wait_for_stable_fingerprint(root, snapshot.fingerprint);
            continue;
        }

        let outcome = super::editor_model::build_captured_snapshot_at(&snapshot, revision)
            .map_err(|failure| failure.envelope);
        let after = project_fingerprint(root);
        if snapshot.fingerprint == after {
            return (outcome, snapshot);
        }
        before = wait_for_stable_fingerprint(root, after);
    }
}

fn project_fingerprint(root: &Path) -> super::editor_model::ProjectSourceFingerprint {
    super::editor_model::capture_project_snapshot(root).fingerprint
}

fn wait_for_stable_fingerprint(
    root: &Path,
    mut observed: super::editor_model::ProjectSourceFingerprint,
) -> super::editor_model::ProjectSourceFingerprint {
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let again = project_fingerprint(root);
        if again == observed {
            return observed;
        }
        observed = again;
    }
}

// ── Play's independent last-good artifacts ─────────────────────────────────

fn recheck_play_into(
    files: &super::editor_model::ProjectSourceFiles,
    state: &RwLock<DevState>,
    clients: &Clients,
) {
    let outcome = recheck_play(files);
    let payload;
    {
        let mut state = state.write().expect("state lock");
        state.play.generation += 1;
        match outcome {
            Ok(good) => {
                state.play.ok = true;
                state.play.diagnostics = None;
                state.play.good = Some(good);
            }
            Err(envelope) => {
                state.play.ok = false;
                state.play.diagnostics = Some(envelope);
            }
        }
        payload = play_sse_payload(&state.play);
    }
    broadcast(clients, &payload);
}

fn recheck_play(
    files: &super::editor_model::ProjectSourceFiles,
) -> Result<GoodBuild, serde_json::Value> {
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
    let input =
        super::editor_model::assemble_snapshot_input(files).map_err(|failure| failure.envelope)?;
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
        ir: program.to_canonical_string(),
        inspect_json: to_canonical_json(&inspection),
        stylesheet: output.stylesheet.clone(),
        fixture_json,
        script_json: script_canonical,
        boot_json: boot_envelope(program, &fixture).map_err(fail)?,
        icons_json: structured_icons_json(),
        config_json,
        provider_js,
    })
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

// ── SSE ────────────────────────────────────────────────────────────────────

type Clients = Arc<Mutex<Vec<Sender<String>>>>;

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
    // Padding crosses tiny_http's write buffer; EventSource ignores comments.
    format!(
        "data: {}\n\n: {}\n\n",
        to_canonical_json(value),
        "·".repeat(4096)
    )
}

fn broadcast(clients: &Clients, payload: &str) {
    let mut clients = clients.lock().expect("clients lock");
    clients.retain(|sender| sender.send(payload.to_string()).is_ok());
}

struct SseStream {
    receiver: Receiver<String>,
    buffer: Vec<u8>,
    offset: usize,
}

impl Read for SseStream {
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

fn respond_sse(request: tiny_http::Request, clients: &Clients, hello: impl FnOnce() -> String) {
    let (sender, receiver) = channel::<String>();
    {
        // Registration and snapshot share the broadcast lock: an update can
        // be duplicated at the boundary but can never be lost.
        let mut clients = clients.lock().expect("clients lock");
        let _ = sender.send(hello());
        clients.push(sender);
    }
    let response = tiny_http::Response::new(
        tiny_http::StatusCode(200),
        vec![
            header("Content-Type", "text/event-stream; charset=utf-8"),
            header("Cache-Control", "no-store"),
        ],
        SseStream {
            receiver,
            buffer: Vec::new(),
            offset: 0,
        },
        None,
        None,
    );
    let _ = request.respond(response);
}

// ── one web application and namespaced transport ───────────────────────────

#[derive(Clone, Debug)]
struct WebApp {
    files: Arc<BTreeMap<String, WebFile>>,
    index: Arc<Vec<u8>>,
    wasm_root: PathBuf,
}

#[derive(Clone, Debug)]
struct WebFile {
    bytes: Arc<Vec<u8>>,
    content_type: String,
}

impl WebApp {
    fn locate() -> Result<Self, String> {
        let mut candidates = Vec::new();
        if let Some(explicit) = std::env::var_os("UHURA_WEB_DIST") {
            candidates.push(PathBuf::from(explicit));
        }
        if let Ok(executable) = std::env::current_exe()
            && let Some(bin) = executable.parent()
        {
            candidates.push(bin.join("../share/uhura/web"));
        }
        candidates.push(tool_root().join("web/dist"));
        load_web_app_from(&candidates)
    }
}

fn load_web_app_from(candidates: &[PathBuf]) -> Result<WebApp, String> {
    let mut attempted = Vec::new();
    for root in candidates {
        if attempted.iter().any(|seen: &PathBuf| seen == root) {
            continue;
        }
        attempted.push(root.clone());
        let index_path = root.join("index.html");
        match std::fs::symlink_metadata(&index_path) {
            Ok(_) => {
                let files = snapshot_web_bundle(root)?;
                let index = files
                    .get("index.html")
                    .ok_or_else(|| format!("{} is not a regular file", index_path.display()))?;
                if index.bytes.is_empty() {
                    return Err(format!("{} is empty", index_path.display()));
                }
                if files.len() == 1 {
                    return Err(format!(
                        "browser application bundle at {} contains only index.html",
                        root.display()
                    ));
                }
                validate_index_assets(root, index.bytes.as_slice(), &files)?;
                let index = Arc::clone(&index.bytes);
                return Ok(WebApp {
                    files: Arc::new(files),
                    index,
                    wasm_root: locate_wasm_for(root),
                });
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

fn locate_wasm_for(web_root: &Path) -> PathBuf {
    if let Some(explicit) = std::env::var_os("UHURA_WASM_DIST") {
        return PathBuf::from(explicit);
    }
    let nested = web_root.join("wasm");
    if nested.is_dir() {
        return nested;
    }
    if let Some(parent) = web_root.parent() {
        let packaged = parent.join("wasm");
        if packaged.is_dir() {
            return packaged;
        }
    }
    tool_root().join("crates/uhura-wasm/pkg/web")
}

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

fn handle(
    request: tiny_http::Request,
    root: &Path,
    web: &WebApp,
    state: &RwLock<DevState>,
    play_clients: &Clients,
    editor_clients: &Clients,
) {
    let url = request.url().to_string();
    let (path, query) = split_request_url(&url);

    if !matches!(
        request.method(),
        tiny_http::Method::Get | tiny_http::Method::Head
    ) {
        let response = tiny_http::Response::from_string("the Uhura host accepts GET and HEAD only")
            .with_status_code(405)
            .with_header(header("Allow", "GET, HEAD"))
            .with_header(header("Content-Type", "text/plain; charset=utf-8"))
            .with_header(header("Cache-Control", "no-store"));
        let _ = request.respond(response);
        return;
    }

    match api_route(path) {
        Some(ApiRoute::PlayEvents) => {
            if request.method() != &tiny_http::Method::Get {
                respond_sse_method_error(request, path);
                return;
            }
            respond_sse(request, play_clients, || {
                play_sse_payload(&state.read().expect("state lock").play)
            });
            return;
        }
        Some(ApiRoute::EditorEvents) => {
            if request.method() != &tiny_http::Method::Get {
                respond_sse_method_error(request, path);
                return;
            }
            respond_sse(request, editor_clients, || {
                let revision = state.read().expect("state lock").editor.source_revision;
                editor_sse_payload(revision)
            });
            return;
        }
        _ => {}
    }

    let outcome = match api_route(path) {
        Some(ApiRoute::EditorState) => editor_state_artifact(state),
        Some(ApiRoute::PlayArtifact(artifact_kind)) => play_artifact(state, artifact_kind),
        Some(ApiRoute::PlayProvider) => provider_artifact(state, query),
        Some(ApiRoute::PlayAsset(relative)) => {
            serve_play_asset(&root.join("fixtures/assets"), relative)
        }
        Some(ApiRoute::PlayWasm(relative)) => {
            serve_tree(&web.wasm_root, relative).map_err(|(status, message)| {
                (
                    status,
                    format!("{message}\n(build the Wasm bundle first: scripts/build-wasm.sh)"),
                )
            })
        }
        Some(ApiRoute::EditorEvents | ApiRoute::PlayEvents) => unreachable!("returned above"),
        Some(ApiRoute::Unknown) => Err((404, format!("no such API endpoint: {path}"))),
        None => application_path(web, path),
    };

    let _ = match outcome {
        Ok((content_type, mut bytes, generation)) => {
            if request.method() == &tiny_http::Method::Head {
                bytes.clear();
            }
            let mut response = tiny_http::Response::from_data(bytes)
                .with_header(header("Content-Type", &content_type))
                .with_header(header("Cache-Control", "no-store"));
            if let Some(generation) = generation {
                response =
                    response.with_header(header("X-Uhura-Generation", &generation.to_string()));
            }
            request.respond(response)
        }
        Err((status, message)) => request.respond(
            tiny_http::Response::from_string(message)
                .with_status_code(tiny_http::StatusCode(status))
                .with_header(header("Content-Type", "text/plain; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store")),
        ),
    };
}

fn respond_sse_method_error(request: tiny_http::Request, path: &str) {
    let response = tiny_http::Response::from_string(format!("{path} requires GET"))
        .with_status_code(405)
        .with_header(header("Allow", "GET"))
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = request.respond(response);
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

fn app_document(web: &WebApp) -> Served {
    Ok((content_type("html"), web.index.as_ref().clone(), None))
}

fn application_path(web: &WebApp, path: &str) -> Served {
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

fn serve_web_file(web: &WebApp, relative: &str) -> Served {
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

fn favicon(web: &WebApp) -> Served {
    match serve_web_file(web, "favicon.ico") {
        Ok(file) => Ok(file),
        Err((404, _)) => Ok((content_type("ico"), Vec::new(), None)),
        Err(error) => Err(error),
    }
}

fn serve_play_asset(base: &Path, encoded_relative: &str) -> Served {
    let relative = decode_play_asset_path(encoded_relative)?;
    serve_tree(base, &relative)
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

fn serve_tree(base: &Path, relative: &str) -> Served {
    if relative
        .split('/')
        .any(|segment| segment == ".." || segment.is_empty())
        || relative.contains('\\')
    {
        return Err((400, "bad path".to_string()));
    }
    let path = base.join(relative);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    std::fs::read(&path)
        .map(|bytes| (content_type(extension), bytes, None))
        .map_err(|error| (404, format!("{}: {error}", path.display())))
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

fn header(name: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use uhura_base::to_canonical_json;
    use uhura_editor_model::{Application, AuthoringMetadata, EditorRender, RenderFreshness};

    use crate::cmd::editor_model::EditorModelArtifact;

    use super::{
        ApiRoute, EditorHostState, PlayArtifact, WebApp, api_route, app_document, application_path,
        content_type, decode_play_asset_path, editor_sse_payload, load_web_app_from, recheck_play,
        serve_play_asset, split_request_url, tool_root,
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
        let web = WebApp {
            files: Arc::new(BTreeMap::new()),
            index: Arc::new(b"<!doctype html><main>Uhura</main>".to_vec()),
            wasm_root: PathBuf::from("unused-wasm"),
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
        let snapshot = super::super::editor_model::capture_project_snapshot(&root);
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
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("uhura-play-assets-{}-{unique}", std::process::id()));
        let assets = root.join("assets");
        fs::create_dir_all(assets.join("summer album")).unwrap();
        fs::write(assets.join("summer album/clip one.mp4"), b"fixture-video").unwrap();
        fs::write(root.join("secret.jpg"), b"outside").unwrap();

        let (kind, bytes, generation) =
            serve_play_asset(&assets, "summer%20album%2Fclip%20one.mp4").unwrap();
        assert_eq!(kind, "video/mp4");
        assert_eq!(bytes, b"fixture-video");
        assert_eq!(generation, None);

        let error = serve_play_asset(&assets, "%252e%252e%2Fsecret.jpg").unwrap_err();
        assert_eq!(error.0, 404);

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
