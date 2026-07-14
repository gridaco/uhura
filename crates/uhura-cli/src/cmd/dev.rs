//! `uhura play [path] [--port <n>]` — the interactive play server (design
//! §12.4). `uhura dev` remains a compatibility alias.
//! tiny_http + SSE. Play watches its source inputs and serves the LAST-GOOD IR
//! plus stylesheet; a failing check pushes diagnostics over `/events`. The
//! Editor additionally rebuilds one complete static Canvas and publishes its
//! independent candidate/active state over `/editor/events`. A rejected Canvas
//! never replaces its last-good document.
//!
//! Endpoints: `/` `/shell/*` `/wasm/*` `/ir.json` `/stylesheet.css`
//! `/fixture.json` `/script.json` `/boot.json` `/icons.json` `/play.json`
//! `/provider.js` `/assets/*` `/events` `/editor/events` (SSE).

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use uhura_base::{Severity, sha256_hex, to_canonical_json, to_envelope};
use uhura_check::check;
use uhura_check::fixture::load_fixture;
use uhura_core::ir::ProgramIr;

use crate::CommonArgs;
use crate::cmd::trace::{boot_updates, fixture_slices_json};

pub fn run(common: &CommonArgs, port: u16) -> ExitCode {
    run_host(common, port, HostEntry::Play, None)
}

/// Host the read-only Canvas and the live Play runtime on one origin. Keeping
/// this in the Play server means `/play` exercises the exact same artifacts,
/// provider, watcher, and SSE path as the dedicated command.
pub(crate) fn run_with_editor(common: &CommonArgs, port: u16, out_dir: Option<&str>) -> ExitCode {
    run_host(common, port, HostEntry::Editor, out_dir.map(PathBuf::from))
}

#[derive(Clone)]
enum HostEntry {
    Play,
    Editor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryDocument {
    Canvas,
    Play,
    EmptyFavicon,
}

impl HostEntry {
    fn command(&self) -> &'static str {
        match self {
            Self::Play => "uhura play",
            Self::Editor => "uhura editor",
        }
    }

    fn document(&self, path: &str) -> Option<EntryDocument> {
        match self {
            Self::Play if path == "/" => Some(EntryDocument::Play),
            Self::Editor => match path {
                "/" | "/index.html" | "/canvas.html" => Some(EntryDocument::Canvas),
                "/play" | "/play/" => Some(EntryDocument::Play),
                "/favicon.ico" => Some(EntryDocument::EmptyFavicon),
                _ => None,
            },
            _ => None,
        }
    }
}

fn run_host(
    common: &CommonArgs,
    port: u16,
    entry: HostEntry,
    editor_out_dir: Option<PathBuf>,
) -> ExitCode {
    let command = entry.command();
    let root = common.root.clone();
    let common = Arc::new(common.clone());
    let state = Arc::new(RwLock::new(DevState {
        generation: 0,
        ok: false,
        diagnostics: None,
        good: None,
        canvas: matches!(entry, HostEntry::Editor).then(CanvasState::default),
    }));
    let play_clients: Clients = Arc::new(Mutex::new(Vec::new()));
    let editor_clients: Clients = Arc::new(Mutex::new(Vec::new()));

    // Scan before the synchronous builds so a save landing during either one
    // is observed. Canvas additionally verifies its content snapshot before
    // accepting the candidate.
    let play_baseline = scan_play_watched(&root);
    let mut canvas_baseline =
        matches!(entry, HostEntry::Editor).then(|| scan_canvas_watched(&root));
    if let Some(seen) = &mut canvas_baseline {
        let (outcome, mut stable) = build_stable_canvas(&common, seen.clone());
        settle_canvas(
            outcome,
            editor_out_dir.as_deref(),
            &root,
            &mut stable,
            &state,
            &editor_clients,
        );
        *seen = stable;
    }
    recheck_into(&root, &state, &play_clients);
    {
        let s = state.read().expect("state lock");
        match (&s.good, s.ok) {
            (Some(_), true) => println!("{command}: checked clean"),
            (Some(_), false) => {
                println!("{command}: check FAILING — serving the last good build")
            }
            (None, _) => println!("{command}: check FAILING — no good build yet (overlay only)"),
        }
    }

    let server = match tiny_http::Server::http(("127.0.0.1", port)) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("{command}: could not bind 127.0.0.1:{port}: {e}");
            return ExitCode::from(2);
        }
    };
    match &entry {
        HostEntry::Play => println!("{command}: http://127.0.0.1:{port}/"),
        HostEntry::Editor => {
            println!("{command}: http://127.0.0.1:{port}/ (read-only; Canvas rebuilds on save)");
            println!("{command}: Play http://127.0.0.1:{port}/play");
        }
    }

    // ── the watcher: poll mtimes, debounce, recheck, broadcast ──────────
    {
        let root = root.clone();
        let common = Arc::clone(&common);
        let state = Arc::clone(&state);
        let play_clients = Arc::clone(&play_clients);
        let editor_clients = Arc::clone(&editor_clients);
        let command = entry.command();
        let editor = matches!(entry, HostEntry::Editor);
        let editor_out_dir = editor_out_dir.clone();
        std::thread::spawn(move || {
            let mut seen_play = play_baseline;
            let mut seen_canvas = canvas_baseline;
            loop {
                std::thread::sleep(Duration::from_millis(150));
                let now_play = scan_play_watched(&root);
                let now_canvas = editor.then(|| scan_canvas_watched(&root));
                let play_changed = now_play != seen_play;
                let canvas_changed = now_canvas != seen_canvas;
                if !play_changed && !canvas_changed {
                    continue;
                }

                // Debounce the complete project observation. Play retains its
                // historical extension boundary; Canvas is conservative and
                // content-addressed.
                let mut stable_play = now_play;
                let mut stable_canvas = now_canvas;
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    let again_play = scan_play_watched(&root);
                    let again_canvas = editor.then(|| scan_canvas_watched(&root));
                    if again_play == stable_play && again_canvas == stable_canvas {
                        break;
                    }
                    stable_play = again_play;
                    stable_canvas = again_canvas;
                }
                let play_changed = stable_play != seen_play;
                let canvas_changed = stable_canvas != seen_canvas;
                seen_play = stable_play;
                seen_canvas = stable_canvas;

                if canvas_changed && let Some(expected) = seen_canvas.take() {
                    let (outcome, mut stable) = build_stable_canvas(&common, expected);
                    settle_canvas(
                        outcome,
                        editor_out_dir.as_deref(),
                        &root,
                        &mut stable,
                        &state,
                        &editor_clients,
                    );
                    seen_canvas = Some(stable);
                    let s = state.read().expect("state lock");
                    let canvas = s.canvas.as_ref().expect("Editor Canvas state");
                    println!(
                        "{command}: Canvas candidate {} — {}",
                        canvas.candidate_generation,
                        if canvas.diagnostics.is_none() {
                            "active, Editor clients converge"
                        } else {
                            "rejected, serving the last good Canvas"
                        }
                    );
                }

                if play_changed {
                    recheck_into(&root, &state, &play_clients);
                    let s = state.read().expect("state lock");
                    println!(
                        "{command}: Play generation {} — {}",
                        s.generation,
                        if s.ok {
                            "ok, shell reloads"
                        } else {
                            "check failing, overlay pushed"
                        }
                    );
                }
            }
        });
    }

    // ── serve: one thread per request (SSE holds its thread) ────────────
    let server = Arc::new(server);
    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        let play_clients = Arc::clone(&play_clients);
        let editor_clients = Arc::clone(&editor_clients);
        let root = root.clone();
        let entry = entry.clone();
        std::thread::spawn(move || {
            handle(
                request,
                &root,
                &state,
                &play_clients,
                &editor_clients,
                &entry,
            )
        });
    }
    ExitCode::SUCCESS
}

// ── state ───────────────────────────────────────────────────────────────────

struct DevState {
    /// Play's latest attempted generation. Canvas has an independent clock.
    generation: u64,
    ok: bool,
    /// `uhura-diagnostics/0` envelope of the latest FAILING check.
    diagnostics: Option<serde_json::Value>,
    /// Last-good artifacts — what every endpoint serves (§12.4).
    good: Option<GoodBuild>,
    /// Present only for the combined Editor host.
    canvas: Option<CanvasState>,
}

struct GoodBuild {
    ir: String,
    stylesheet: String,
    fixture_json: String,
    script_json: String,
    boot_json: String,
    icons_json: String,
    /// Browser play selection. The fixture remains the native test double;
    /// a module provider replaces it only in the `uhura play` shell.
    play_json: String,
    /// Provider module bytes captured with the rest of the last-good build.
    provider_js: Option<String>,
}

#[derive(Default)]
struct CanvasState {
    /// Advances for every settled Canvas build, including rejected builds.
    candidate_generation: u64,
    /// The accepted candidate whose document is currently served.
    active_generation: Option<u64>,
    /// SHA-256 of the exact base self-contained HTML. Unlike the numeric
    /// counter, this remains meaningful across server process restarts.
    active_build_id: Option<String>,
    /// Diagnostics for the latest rejected candidate.
    diagnostics: Option<serde_json::Value>,
    /// Non-fatal checker warnings belonging to the active generation.
    active_warnings: Option<serde_json::Value>,
    /// Base self-contained Canvas HTML. Editor-only host metadata is injected
    /// at response time so it cannot leak into `uhura project` exports.
    good_html: Option<String>,
}

impl CanvasState {
    fn settle(&mut self, outcome: Result<(String, Option<serde_json::Value>), serde_json::Value>) {
        self.candidate_generation += 1;
        match outcome {
            Ok((html, warnings)) => {
                self.active_build_id = Some(sha256_hex(html.as_bytes()));
                self.active_generation = Some(self.candidate_generation);
                self.diagnostics = None;
                self.active_warnings = warnings;
                self.good_html = Some(html);
            }
            Err(diagnostics) => {
                self.diagnostics = Some(diagnostics);
            }
        }
    }
}

type Clients = Arc<Mutex<Vec<Sender<String>>>>;

/// (content-type, body, generation stamp, Canvas build identity) or
/// (status, message).
type Served = Result<(String, Vec<u8>, Option<u64>, Option<String>), (u16, String)>;

/// Build until the project content observed before and after projection is
/// identical. A candidate assembled while another save lands is discarded
/// before it can become active.
fn build_stable_canvas(
    common: &CommonArgs,
    mut before: super::project::CanvasSourceFingerprint,
) -> (
    Result<super::project::CanvasArtifact, super::project::CanvasBuildFailure>,
    super::project::CanvasSourceFingerprint,
) {
    loop {
        let snapshot = super::project::capture_canvas_snapshot(&common.root);
        if before != snapshot.fingerprint {
            before = snapshot.fingerprint;
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let again = scan_canvas_watched(&common.root);
                if again == before {
                    break;
                }
                before = again;
            }
            continue;
        }

        let outcome = super::project::build_captured_snapshot(&snapshot);
        let after = scan_canvas_watched(&common.root);
        if snapshot.fingerprint == after {
            return (outcome, after);
        }

        before = after;
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let again = scan_canvas_watched(&common.root);
            if again == before {
                break;
            }
            before = again;
        }
    }
}

/// Filesystem identity of one pre-existing logical path to the configured
/// export. The canonical target catches retargeted parent-directory symlinks;
/// the optional link target catches replacement/retargeting of the final path.
#[derive(Clone, Debug, Eq, PartialEq)]
struct CanvasOutputAliasIdentity {
    canonical_target: PathBuf,
    symlink_target: Option<PathBuf>,
}

impl CanvasOutputAliasIdentity {
    fn observe(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if !metadata.is_file() && !metadata.file_type().is_symlink() {
            return None;
        }
        let symlink_target = metadata
            .file_type()
            .is_symlink()
            .then(|| std::fs::read_link(path).ok())
            .flatten();
        if metadata.file_type().is_symlink() && symlink_target.is_none() {
            return None;
        }
        let canonical_target = std::fs::canonicalize(path).ok().or_else(|| {
            symlink_target
                .as_ref()
                .and_then(|_| prospective_canonical_output(path))
        })?;
        Some(Self {
            canonical_target,
            symlink_target,
        })
    }

    fn matches_fingerprint(&self, fingerprint: &str) -> bool {
        match &self.symlink_target {
            Some(target) => {
                let target = format!("{:?}", target.as_os_str());
                fingerprint.starts_with(&format!("!symlink-file:{target}:"))
                    || fingerprint.starts_with(&format!("!symlink-unresolved:{target}:"))
            }
            None => {
                fingerprint.len() == 64 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
            }
        }
    }

    fn fingerprint(&self, digest: &str) -> String {
        match &self.symlink_target {
            Some(target) => format!("!symlink-file:{:?}:{digest}", target.as_os_str()),
            None => digest.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
struct CanvasOutputAliasPatch {
    path: PathBuf,
    identity: CanvasOutputAliasIdentity,
}

/// Authorization captured before an export write. Only paths that already
/// resolved to the destination in the accepted candidate, plus the exact
/// lexical/canonical files that this write can create, are eligible. Applying
/// the token never imports arbitrary post-write aliases or bytes.
#[derive(Clone, Debug)]
struct CanvasOutputBaselinePatch {
    canonical_target: PathBuf,
    aliases: Vec<CanvasOutputAliasPatch>,
    created_paths: Vec<PathBuf>,
    destination_absent: bool,
}

impl CanvasOutputBaselinePatch {
    fn capture(
        root: &Path,
        destination: &Path,
        baseline: &super::project::CanvasSourceFingerprint,
    ) -> Option<Self> {
        let logical_root = absolute_logical_path(root)?;
        let root = super::project::canvas_scan_root(root);
        let absolute_destination = absolute_logical_path(destination)?;
        let destination = absolute_destination
            .strip_prefix(&logical_root)
            .map(|relative| root.join(relative))
            .unwrap_or(absolute_destination);
        let canonical_target = prospective_canonical_output(&destination)?;
        if !canonical_target.starts_with(&root) {
            return None;
        }

        let aliases = baseline
            .iter()
            .filter_map(|(path, fingerprint)| {
                let identity = CanvasOutputAliasIdentity::observe(path)?;
                (identity.canonical_target == canonical_target
                    && identity.matches_fingerprint(fingerprint))
                .then(|| CanvasOutputAliasPatch {
                    path: path.clone(),
                    identity,
                })
            })
            .collect::<Vec<_>>();

        let destination_absent = matches!(
            std::fs::symlink_metadata(&destination),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        );
        let mut created_paths = Vec::new();
        for path in [&destination, &canonical_target] {
            if !baseline.contains_key(path)
                && matches!(
                    std::fs::symlink_metadata(path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound
                )
            {
                created_paths.push(path.clone());
            }
        }
        created_paths.sort();
        created_paths.dedup();

        Some(Self {
            canonical_target,
            aliases,
            created_paths,
            destination_absent,
        })
    }

    fn apply(
        self,
        root: &Path,
        bytes: &[u8],
        baseline: &mut super::project::CanvasSourceFingerprint,
    ) {
        let digest = sha256_hex(bytes);
        let current = scan_canvas_watched(root);

        for alias in self.aliases {
            let expected = alias.identity.fingerprint(&digest);
            if CanvasOutputAliasIdentity::observe(&alias.path) == Some(alias.identity)
                && current.get(&alias.path) == Some(&expected)
            {
                baseline.insert(alias.path, expected);
            }
        }

        let created_identity = CanvasOutputAliasIdentity {
            canonical_target: self.canonical_target,
            symlink_target: None,
        };
        for path in self.created_paths {
            if CanvasOutputAliasIdentity::observe(&path) == Some(created_identity.clone())
                && current.get(&path) == Some(&digest)
            {
                baseline.insert(path, digest.clone());
            }
        }
    }
}

fn absolute_logical_path(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        std::env::current_dir()
            .ok()
            .map(|current| current.join(path))
    }
}

fn prospective_canonical_output(path: &Path) -> Option<PathBuf> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Some(canonical);
    }
    if std::fs::symlink_metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        let target = std::fs::read_link(path).ok()?;
        let target = if target.is_absolute() {
            target
        } else {
            path.parent()?.join(target)
        };
        return target.parent().and_then(|parent| {
            std::fs::canonicalize(parent)
                .ok()
                .and_then(|parent| target.file_name().map(|name| parent.join(name)))
        });
    }
    path.parent().and_then(|parent| {
        std::fs::canonicalize(parent)
            .ok()
            .and_then(|parent| path.file_name().map(|name| parent.join(name)))
    })
}

fn settle_canvas(
    outcome: Result<super::project::CanvasArtifact, super::project::CanvasBuildFailure>,
    out_dir: Option<&Path>,
    root: &Path,
    baseline: &mut super::project::CanvasSourceFingerprint,
    state: &RwLock<DevState>,
    clients: &Clients,
) {
    let outcome = match outcome {
        Ok(artifact) => {
            let mut destination = String::new();
            if let Some(dir) = out_dir {
                let path = dir.join("canvas.html");
                let export = std::fs::create_dir_all(dir).and_then(|()| {
                    let patch = CanvasOutputBaselinePatch::capture(root, &path, baseline);
                    let destination_absent =
                        patch.as_ref().is_some_and(|patch| patch.destination_absent);
                    write_canvas_output(&path, artifact.html.as_bytes(), destination_absent)?;
                    if let Some(patch) = patch {
                        patch.apply(root, artifact.html.as_bytes(), baseline);
                    }
                    Ok(())
                });
                match export {
                    Ok(()) => destination = format!(" and exported to {}", path.display()),
                    Err(error) => eprintln!(
                        "uhura editor: could not export {}: {error}; the in-memory Canvas remains active",
                        path.display()
                    ),
                }
            }
            println!(
                "uhura editor: projected {} previews ({} replay-derived) in memory{}",
                artifact.preview_count, artifact.replay_derived_count, destination
            );
            Ok((artifact.html, artifact.warnings))
        }
        Err(failure) => Err(failure.envelope),
    };

    let payload;
    {
        let mut s = state.write().expect("state lock");
        let canvas = s.canvas.as_mut().expect("Editor Canvas state");
        canvas.settle(outcome);
        payload = canvas_sse_payload(canvas);
    }
    broadcast(clients, &payload);
}

fn write_canvas_output(path: &Path, bytes: &[u8], create_new: bool) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)
}

/// One recheck: run the pure pipeline, swap the state, broadcast the SSE
/// event. A failing check NEVER clobbers the last good build.
fn recheck_into(root: &Path, state: &RwLock<DevState>, clients: &Clients) {
    let outcome = recheck(root);
    let payload;
    {
        let mut s = state.write().expect("state lock");
        s.generation += 1;
        match outcome {
            Ok(good) => {
                s.ok = true;
                s.diagnostics = None;
                s.good = Some(good);
            }
            Err(envelope) => {
                s.ok = false;
                s.diagnostics = Some(envelope);
            }
        }
        payload = sse_payload(&s);
    }
    broadcast(clients, &payload);
}

fn recheck(root: &Path) -> Result<GoodBuild, serde_json::Value> {
    let fail = |message: String| {
        serde_json::json!({
            "format": "uhura-diagnostics",
            "version": 0,
            "diagnostics": [{
                "code": "UH9000",
                "rule": "play/recheck",
                "severity": "error",
                "message": message,
            }],
        })
    };
    let input = crate::cmd::assemble_input(root)
        .map_err(|_| fail("the corpus could not be read (see the server log)".to_string()))?;
    let output = check(&input);
    if output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(to_envelope(&output.diagnostics, &output.source_map));
    }
    let Some(lowered) = &output.lowered else {
        return Err(fail("the check produced no program".to_string()));
    };
    let program = &lowered.program;

    // The play profile names the fixture + script (§3).
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
    let read = |rel: &str| -> Result<String, serde_json::Value> {
        std::fs::read_to_string(root.join(rel)).map_err(|e| fail(format!("{rel}: {e}")))
    };
    let fixture_text = read(fixture_rel)?;
    let script_text = read(&format!("fixtures/scripts/{}.toml", profile.script))?;
    let fixture = load_fixture(&fixture_text)
        .map_err(|issues| fail(format!("fixture: {}", issues[0].message)))?;
    let script_json = uhura_fixture::toml_to_json(&script_text).map_err(fail)?;
    let fixture_json = fixture_slices_json(&fixture);
    let script_canonical = to_canonical_json(&script_json);
    // An "ok" generation must be BOOT-VIABLE: the driver's strict grammar
    // (slice refs, closed script fields) validates here, not in the
    // browser after a reload.
    uhura_fixture::FixtureDriver::new(&fixture_json, &script_canonical)
        .map_err(|e| fail(format!("script `{}`: {e}", profile.script)))?;

    let (play_json, provider_js) = match &profile.provider {
        Some(provider) => {
            let provider_js = read(&provider.module)?;
            let provider_hash = sha256_hex(provider_js.as_bytes());
            let play_json = to_canonical_json(&serde_json::json!({
                "allow_fixture": profile.allow_fixture,
                "provider": {
                    "kind": "module",
                    "module": format!("/provider.js?sha256={provider_hash}"),
                    "config": &provider.config,
                },
            }));
            (play_json, Some(provider_js))
        }
        None => (
            to_canonical_json(&serde_json::json!({
                "allow_fixture": true,
                "provider": { "kind": "fixture" },
            })),
            None,
        ),
    };

    Ok(GoodBuild {
        ir: program.to_canonical_string(),
        stylesheet: output.stylesheet.clone(),
        fixture_json,
        script_json: script_canonical,
        boot_json: boot_envelope(program, &fixture).map_err(fail)?,
        icons_json: icons_json(root, &manifest.catalog_path),
        play_json,
        provider_js,
    })
}

/// The `/boot.json` wire form — `{"updates": […]}` over `boot_updates`.
/// The acceptance battery's parity artifacts reuse it so the file the
/// wasm side boots from has exactly one producer.
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

/// The catalog's closed icon set → inline SVG glyph table. One source of
/// truth: the same `uhura_project::icons` the static canvas embeds.
fn icons_json(root: &Path, catalog_rel: &str) -> String {
    let mut glyphs = serde_json::Map::new();
    if let Ok(text) = std::fs::read_to_string(root.join(catalog_rel))
        && let Ok(value) = text.parse::<toml::Value>()
        && let Some(icons) = value
            .get("catalog")
            .and_then(|c| c.get("icons"))
            .and_then(|i| i.as_array())
    {
        for name in icons.iter().filter_map(|n| n.as_str()) {
            if let Some(glyph) = uhura_project::icons::glyph(name) {
                glyphs.insert(name.to_string(), serde_json::Value::String(glyph.into()));
            }
        }
    }
    to_canonical_json(&serde_json::Value::Object(glyphs))
}

// ── the watcher's view of the tree ──────────────────────────────────────────

/// Play retains its existing saved-source observation boundary. Static Canvas
/// observation below is deliberately broader and content-addressed.
fn scan_play_watched(root: &Path) -> BTreeMap<PathBuf, (SystemTime, u64)> {
    let mut out = BTreeMap::new();
    scan_play_dir(root, &mut out);
    out
}

fn scan_play_dir(dir: &Path, out: &mut BTreeMap<PathBuf, (SystemTime, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !matches!(
                name.as_ref(),
                "build" | "renders" | "target" | "node_modules" | ".git"
            ) {
                scan_play_dir(&path, out);
            }
            continue;
        }
        let watched = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("uhura" | "toml" | "css" | "js")
        ) && name != "uhura.lock";
        if watched && let Ok(meta) = entry.metadata() {
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            out.insert(path, (mtime, meta.len()));
        }
    }
}

/// The initial live-Canvas implementation favors completeness over a derived
/// dependency graph: every non-generated project file is content-fingerprinted
/// so fixtures, locks, catalogs, and binary media all invalidate projection.
fn scan_canvas_watched(root: &Path) -> super::project::CanvasSourceFingerprint {
    super::project::capture_canvas_snapshot(root).fingerprint
}

// ── SSE ─────────────────────────────────────────────────────────────────────

fn sse_payload(s: &DevState) -> String {
    let mut event = serde_json::json!({
        "generation": s.generation,
        "ok": s.ok,
    });
    if let Some(diagnostics) = &s.diagnostics {
        event["diagnostics"] = diagnostics.clone();
    }
    // Trailing SSE comment pads past tiny_http's ~8 KiB write buffer so
    // the data frame flushes NOW — EventSource ignores comment lines.
    format!(
        "data: {}\n\n: {}\n\n",
        to_canonical_json(&event),
        "·".repeat(4096)
    )
}

fn canvas_sse_payload(canvas: &CanvasState) -> String {
    let mut event = serde_json::json!({
        "candidateGeneration": canvas.candidate_generation,
        "activeGeneration": canvas.active_generation,
        "activeBuildId": canvas.active_build_id,
        "status": if canvas.diagnostics.is_some() { "rejected" } else { "active" },
    });
    if let Some(diagnostics) = &canvas.diagnostics {
        event["diagnostics"] = diagnostics.clone();
    } else if let Some(warnings) = &canvas.active_warnings {
        event["diagnostics"] = warnings.clone();
    }
    format!(
        "data: {}\n\n: {}\n\n",
        to_canonical_json(&event),
        "·".repeat(4096)
    )
}

fn broadcast(clients: &Clients, payload: &str) {
    let mut clients = clients.lock().expect("clients lock");
    clients.retain(|tx| tx.send(payload.to_string()).is_ok());
}

/// Blocking channel-backed body: each `read` hands out the next pushed
/// SSE frame; the response thread parks in `recv` between events.
struct SseStream {
    rx: Receiver<String>,
    buffer: Vec<u8>,
    offset: usize,
}

impl Read for SseStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.offset >= self.buffer.len() {
            match self.rx.recv() {
                Ok(frame) => {
                    self.buffer = frame.into_bytes();
                    self.offset = 0;
                }
                Err(_) => return Ok(0), // server side dropped: EOF
            }
        }
        let n = (self.buffer.len() - self.offset).min(buf.len());
        buf[..n].copy_from_slice(&self.buffer[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}

// ── request handling ────────────────────────────────────────────────────────

/// The uhura workspace root — the compiled web host and wasm bundle live in the
/// TOOLCHAIN tree, not the corpus (compile-time anchored; `uhura play`
/// is a dev tool run from this repo).
fn tool_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn handle(
    request: tiny_http::Request,
    root: &Path,
    state: &RwLock<DevState>,
    play_clients: &Clients,
    editor_clients: &Clients,
    entry: &HostEntry,
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

    if path == "/events" {
        if request.method() != &tiny_http::Method::Get {
            let response = tiny_http::Response::from_string("/events requires GET")
                .with_status_code(405)
                .with_header(header("Allow", "GET"))
                .with_header(header("Content-Type", "text/plain; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store"));
            let _ = request.respond(response);
            return;
        }
        let (tx, rx) = channel::<String>();
        {
            // Registration and the hello snapshot happen under the same
            // lock `broadcast` takes, so no generation can slip between
            // them (it lands as a duplicate, never a gap).
            let mut clients = play_clients.lock().expect("clients lock");
            let hello = sse_payload(&state.read().expect("state lock"));
            let _ = tx.send(hello);
            clients.push(tx);
        }
        let response = tiny_http::Response::new(
            tiny_http::StatusCode(200),
            vec![
                header("Content-Type", "text/event-stream; charset=utf-8"),
                header("Cache-Control", "no-store"),
            ],
            SseStream {
                rx,
                buffer: Vec::new(),
                offset: 0,
            },
            None,
            None,
        );
        let _ = request.respond(response); // blocks for the client's lifetime
        return;
    }

    if path == "/editor/events" && matches!(entry, HostEntry::Editor) {
        if request.method() != &tiny_http::Method::Get {
            let response = tiny_http::Response::from_string("/editor/events requires GET")
                .with_status_code(405)
                .with_header(header("Allow", "GET"))
                .with_header(header("Content-Type", "text/plain; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store"));
            let _ = request.respond(response);
            return;
        }
        let (tx, rx) = channel::<String>();
        {
            // As with Play, registration and hello share the broadcast lock:
            // an activation can duplicate but never slip through the gap.
            let mut clients = editor_clients.lock().expect("Editor clients lock");
            let s = state.read().expect("state lock");
            let canvas = s.canvas.as_ref().expect("Editor Canvas state");
            let _ = tx.send(canvas_sse_payload(canvas));
            clients.push(tx);
        }
        let response = tiny_http::Response::new(
            tiny_http::StatusCode(200),
            vec![
                header("Content-Type", "text/event-stream; charset=utf-8"),
                header("Cache-Control", "no-store"),
            ],
            SseStream {
                rx,
                buffer: Vec::new(),
                offset: 0,
            },
            None,
            None,
        );
        let _ = request.respond(response);
        return;
    }

    let outcome: Served = match entry.document(path) {
        Some(EntryDocument::Canvas) => canvas_document(state),
        Some(EntryDocument::Play) => play_document(matches!(entry, HostEntry::Editor)),
        Some(EntryDocument::EmptyFavicon) => Ok((ct("ico"), Vec::new(), None, None)),
        None => match path {
            "/ir.json" => artifact(state, |g| g.ir.clone().into_bytes(), "json"),
            "/stylesheet.css" => artifact(state, |g| g.stylesheet.clone().into_bytes(), "css"),
            "/fixture.json" => artifact(state, |g| g.fixture_json.clone().into_bytes(), "json"),
            "/script.json" => artifact(state, |g| g.script_json.clone().into_bytes(), "json"),
            "/boot.json" => artifact(state, |g| g.boot_json.clone().into_bytes(), "json"),
            "/icons.json" => artifact(state, |g| g.icons_json.clone().into_bytes(), "json"),
            "/play.json" => artifact(state, |g| g.play_json.clone().into_bytes(), "json"),
            "/provider.js" => provider_artifact(state, query),
            _ => {
                if let Some(rel) = path.strip_prefix("/shell/") {
                    serve_tree(&tool_root().join("web/dist/play"), rel)
                } else if let Some(rel) = path.strip_prefix("/wasm/") {
                    serve_tree(&tool_root().join("crates/uhura-wasm/pkg/web"), rel).map_err(
                        |(code, msg)| {
                            (
                                code,
                                format!("{msg}\n(build the bundle first: scripts/build-wasm.sh)"),
                            )
                        },
                    )
                } else if let Some(rel) = path.strip_prefix("/assets/") {
                    serve_tree(&root.join("fixtures/assets"), rel)
                } else {
                    Err((404, format!("no such endpoint: {path}")))
                }
            }
        },
    };

    let _ = match outcome {
        Ok((content_type, mut bytes, generation, build_id)) => {
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
            if let Some(build_id) = build_id {
                response = response.with_header(header("X-Uhura-Build", &build_id));
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

fn split_request_url(url: &str) -> (&str, Option<&str>) {
    url.split_once('?')
        .map_or((url, None), |(path, query)| (path, Some(query)))
}

fn canvas_document(state: &RwLock<DevState>) -> Served {
    let s = state.read().expect("state lock");
    let canvas = s
        .canvas
        .as_ref()
        .expect("Canvas route requires Editor mode");
    match (
        &canvas.good_html,
        canvas.active_generation,
        &canvas.active_build_id,
    ) {
        (Some(html), Some(generation), Some(build_id)) => Ok((
            ct("html"),
            super::editor::editor_html(html, generation, build_id).into_bytes(),
            Some(generation),
            Some(build_id.clone()),
        )),
        _ => Ok((
            ct("html"),
            super::editor::cold_html().into_bytes(),
            None,
            None,
        )),
    }
}

/// The editor's `/play` document differs from dedicated Play by one host-owned
/// return link. The runtime scripts and every endpoint remain identical.
fn play_document(with_editor_navigation: bool) -> Served {
    let bytes = file_bytes(&tool_root().join("web/dist/play/index.html"))?;
    if !with_editor_navigation {
        return Ok((ct("html"), bytes, None, None));
    }
    let shell = String::from_utf8(bytes).map_err(|error| {
        (
            500,
            format!("web/dist/play/index.html is not UTF-8: {error}"),
        )
    })?;
    Ok((
        ct("html"),
        super::editor::play_html(&shell).into_bytes(),
        None,
        None,
    ))
}

/// A last-good in-memory artifact; 503 + the failure story before the
/// first clean check. The generation stamp lets the shell detect a
/// recheck landing between its artifact fetches (mixed-build boot).
fn artifact(state: &RwLock<DevState>, pick: impl Fn(&GoodBuild) -> Vec<u8>, ext: &str) -> Served {
    let s = state.read().expect("state lock");
    match &s.good {
        Some(good) => Ok((ct(ext), pick(good), Some(s.generation), None)),
        None => Err((
            503,
            "no good build yet — fix the check errors (the overlay lists them)".to_string(),
        )),
    }
}

/// The configured provider module, captured in the same last-good generation
/// as the IR and play config. Fixture-backed profiles intentionally have no
/// module endpoint.
fn provider_artifact(state: &RwLock<DevState>, query: Option<&str>) -> Served {
    let s = state.read().expect("state lock");
    match &s.good {
        Some(good) => match &good.provider_js {
            Some(module) => {
                let requested_hash = query.and_then(|query| {
                    query
                        .split('&')
                        .find_map(|part| part.strip_prefix("sha256="))
                });
                let actual_hash = sha256_hex(module.as_bytes());
                if requested_hash.is_some_and(|expected| expected != actual_hash) {
                    return Err((
                        409,
                        "the provider changed after play.json was fetched — reload the page"
                            .to_string(),
                    ));
                }
                Ok((
                    ct("js"),
                    module.clone().into_bytes(),
                    Some(s.generation),
                    None,
                ))
            }
            None => Err((
                404,
                "the play profile uses the fixture provider".to_string(),
            )),
        },
        None => Err((
            503,
            "no good build yet — fix the check errors (the overlay lists them)".to_string(),
        )),
    }
}

/// One file under a served tree; refuses traversal.
fn serve_tree(base: &Path, rel: &str) -> Served {
    if rel.split('/').any(|seg| seg == ".." || seg.is_empty()) || rel.contains('\\') {
        return Err((400, "bad path".to_string()));
    }
    let path = base.join(rel);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    file_bytes(&path).map(|b| (ct(ext), b, None, None))
}

fn file_bytes(path: &Path) -> Result<Vec<u8>, (u16, String)> {
    std::fs::read(path).map_err(|e| (404, format!("{}: {e}", path.display())))
}

fn ct(ext: &str) -> String {
    match ext {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "jpg" | "jpeg" => "image/jpeg",
        "mp4" => "video/mp4",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn header(name: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::RwLock;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use uhura_base::sha256_hex;

    use crate::CommonArgs;
    use crate::cmd::project::{CanvasSourceFingerprint, capture_canvas_snapshot};

    use super::{
        CanvasOutputBaselinePatch, CanvasState, DevState, EntryDocument, HostEntry,
        canvas_document, canvas_sse_payload, ct, play_document, scan_canvas_watched, settle_canvas,
        split_request_url, write_canvas_output,
    };

    fn export_and_patch(
        root: &std::path::Path,
        out: &std::path::Path,
        bytes: &[u8],
        baseline: &mut CanvasSourceFingerprint,
    ) -> std::io::Result<()> {
        fs::create_dir_all(out)?;
        let path = out.join("canvas.html");
        let patch = CanvasOutputBaselinePatch::capture(root, &path, baseline);
        let create_new = patch.as_ref().is_some_and(|patch| patch.destination_absent);
        write_canvas_output(&path, bytes, create_new)?;
        if let Some(patch) = patch {
            patch.apply(root, bytes, baseline);
        }
        Ok(())
    }

    #[test]
    fn editor_and_dedicated_play_have_distinct_entry_routes() {
        let editor = HostEntry::Editor;
        assert_eq!(editor.document("/"), Some(EntryDocument::Canvas));
        assert_eq!(editor.document("/canvas.html"), Some(EntryDocument::Canvas));
        assert_eq!(editor.document("/play"), Some(EntryDocument::Play));
        assert_eq!(editor.document("/play/"), Some(EntryDocument::Play));

        let play = HostEntry::Play;
        assert_eq!(play.document("/"), Some(EntryDocument::Play));
        assert_eq!(play.document("/play"), None);
    }

    #[test]
    fn app_query_is_separate_from_the_editor_play_route() {
        let (path, query) = split_request_url("/play?post=42&compose=open");
        assert_eq!(path, "/play");
        assert_eq!(query, Some("post=42&compose=open"));
    }

    #[test]
    fn only_editor_hosted_play_gets_return_navigation() {
        let (_, dedicated, _, _) = play_document(false).expect("dedicated Play document");
        let (_, editor, _, _) = play_document(true).expect("editor-hosted Play document");
        let dedicated = String::from_utf8(dedicated).expect("UTF-8 shell");
        let editor = String::from_utf8(editor).expect("UTF-8 shell");

        assert!(dedicated.contains("<!-- uhura-editor-navigation -->"));
        assert!(!dedicated.contains("Return to Uhura Editor"));
        assert!(!editor.contains("uhura-editor-navigation"));
        assert!(editor.contains("Return to Uhura Editor"));
    }

    #[test]
    fn fixture_video_assets_are_served_as_mp4() {
        assert_eq!(ct("mp4"), "video/mp4");
    }

    #[test]
    fn rejected_canvas_candidate_retains_the_active_document_and_generation() {
        let mut canvas = CanvasState::default();
        canvas.settle(Ok(("Canvas A".to_string(), None)));
        canvas.settle(Err(serde_json::json!({ "diagnostics": ["broken B"] })));

        assert_eq!(canvas.candidate_generation, 2);
        assert_eq!(canvas.active_generation, Some(1));
        assert_eq!(
            canvas.active_build_id.as_deref(),
            Some(sha256_hex(b"Canvas A").as_str())
        );
        assert_eq!(canvas.good_html.as_deref(), Some("Canvas A"));
        assert!(canvas.diagnostics.is_some());
    }

    #[test]
    fn valid_canvas_after_rejection_activates_the_new_candidate() {
        let mut canvas = CanvasState::default();
        canvas.settle(Err(serde_json::json!({ "diagnostics": ["broken A"] })));
        canvas.settle(Ok(("Canvas B".to_string(), None)));

        assert_eq!(canvas.candidate_generation, 2);
        assert_eq!(canvas.active_generation, Some(2));
        assert_eq!(
            canvas.active_build_id.as_deref(),
            Some(sha256_hex(b"Canvas B").as_str())
        );
        assert_eq!(canvas.good_html.as_deref(), Some("Canvas B"));
        assert!(canvas.diagnostics.is_none());
    }

    #[test]
    fn canvas_build_identity_is_content_derived_across_process_local_counters() {
        let mut first_process = CanvasState::default();
        first_process.settle(Err(serde_json::json!({ "diagnostics": ["broken"] })));
        first_process.settle(Ok(("same Canvas".to_string(), None)));

        let mut restarted_process = CanvasState::default();
        restarted_process.settle(Ok(("same Canvas".to_string(), None)));

        assert_ne!(
            first_process.active_generation,
            restarted_process.active_generation
        );
        assert_eq!(
            first_process.active_build_id,
            restarted_process.active_build_id
        );

        restarted_process.settle(Ok(("different Canvas".to_string(), None)));
        assert_ne!(
            first_process.active_build_id,
            restarted_process.active_build_id
        );
    }

    #[test]
    fn editor_event_distinguishes_candidate_from_active_generation() {
        let mut canvas = CanvasState::default();
        canvas.settle(Ok(("Canvas A".to_string(), None)));
        canvas.settle(Err(serde_json::json!({
            "format": "uhura-diagnostics",
            "diagnostics": [{ "message": "broken B" }],
        })));

        let frame = canvas_sse_payload(&canvas);
        let json = frame
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("data: "))
            .expect("SSE data line");
        let event: serde_json::Value = serde_json::from_str(json).expect("event JSON");

        assert_eq!(event["candidateGeneration"], 2);
        assert_eq!(event["activeGeneration"], 1);
        assert_eq!(event["activeBuildId"], sha256_hex(b"Canvas A"));
        assert_eq!(event["status"], "rejected");
        assert_eq!(event["diagnostics"]["format"], "uhura-diagnostics");
    }

    #[test]
    fn active_canvas_event_retains_its_non_fatal_warnings() {
        let warnings = serde_json::json!({
            "format": "uhura-diagnostics",
            "version": 0,
            "summary": { "errors": 0, "warnings": 1 },
            "diagnostics": [{ "severity": "warning", "message": "check this" }],
        });
        let mut canvas = CanvasState::default();
        canvas.settle(Ok(("Canvas A".to_string(), Some(warnings.clone()))));

        let frame = canvas_sse_payload(&canvas);
        let json = frame
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("data: "))
            .expect("SSE data line");
        let event: serde_json::Value = serde_json::from_str(json).expect("event JSON");

        assert_eq!(event["status"], "active");
        assert_eq!(event["candidateGeneration"], event["activeGeneration"]);
        assert_eq!(event["diagnostics"], warnings);
    }

    #[test]
    fn canvas_document_serves_cold_recovery_then_the_exact_active_generation() {
        let state = RwLock::new(DevState {
            generation: 0,
            ok: false,
            diagnostics: None,
            good: None,
            canvas: Some(CanvasState::default()),
        });
        let (_, cold, cold_generation, cold_build_id) =
            canvas_document(&state).expect("cold document");
        let cold = String::from_utf8(cold).expect("cold HTML");
        assert_eq!(cold_generation, None);
        assert_eq!(cold_build_id, None);
        assert!(cold.contains("name=\"uhura-editor-host\" content=\"0\""));
        assert!(cold.contains("name=\"uhura-editor-build\" content=\"\""));

        state
            .write()
            .expect("state")
            .canvas
            .as_mut()
            .expect("Canvas")
            .settle(Ok((
                "<html><head><title>Demo — uhura canvas</title></head><body><!-- uhura-editor-actions --></body></html>"
                    .to_string(),
                None,
            )));
        let (_, active, active_generation, active_build_id) =
            canvas_document(&state).expect("active document");
        let active = String::from_utf8(active).expect("active HTML");
        let expected_build_id = state
            .read()
            .expect("state")
            .canvas
            .as_ref()
            .expect("Canvas")
            .active_build_id
            .clone();
        assert_eq!(active_generation, Some(1));
        assert_eq!(active_build_id, expected_build_id);
        assert!(active.contains("name=\"uhura-editor-host\" content=\"1\""));
        assert!(active.contains(&format!(
            "name=\"uhura-editor-build\" content=\"{}\"",
            active_build_id.expect("active build ID")
        )));
    }

    #[test]
    fn canvas_observation_includes_lock_and_binary_assets_but_not_outputs() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-canvas-watch-{}-{unique}",
            std::process::id()
        ));
        let assets = root.join("fixtures/assets");
        let nested_build = root.join("components/build");
        let nested_target = root.join("fixtures/target");
        let nested_dependencies = root.join("fixtures/node_modules/package");
        let root_build = root.join("build");
        let renders = root.join("renders");
        fs::create_dir_all(&assets).expect("asset directory");
        fs::create_dir_all(&nested_build).expect("nested source directory");
        fs::create_dir_all(&nested_target).expect("nested fixture directory");
        fs::create_dir_all(&nested_dependencies).expect("nested dependency directory");
        fs::create_dir_all(&root_build).expect("root build directory");
        fs::create_dir_all(&renders).expect("renders directory");
        fs::write(root.join("uhura.lock"), "locked").expect("lock");
        fs::write(assets.join("photo.jpg"), [1_u8, 2, 3]).expect("asset");
        fs::write(nested_build.join("card.uhura"), "component card").expect("nested source");
        fs::write(nested_target.join("feed.toml"), "items = []").expect("nested fixture");
        fs::write(nested_dependencies.join("ignored.toml"), "generated = true")
            .expect("nested dependency");
        fs::write(root_build.join("ir.json"), "generated").expect("root build output");
        fs::write(renders.join("canvas.html"), "generated").expect("render");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&root, root.join("cycle")).expect("directory symlink");

        let observed_root = fs::canonicalize(&root).expect("canonical test root");
        let before = scan_canvas_watched(&root);
        assert!(before.contains_key(&observed_root.join("uhura.lock")));
        assert!(before.contains_key(&observed_root.join("fixtures/assets/photo.jpg")));
        assert!(before.contains_key(&observed_root.join("components/build/card.uhura")));
        assert!(before.contains_key(&observed_root.join("fixtures/target/feed.toml")));
        assert!(!before.contains_key(&observed_root.join("build/ir.json")));
        assert!(!before.contains_key(&observed_root.join("renders/canvas.html")));
        assert!(
            !before.contains_key(&observed_root.join("fixtures/node_modules/package/ignored.toml"))
        );
        #[cfg(unix)]
        {
            let cycle = observed_root.join("cycle");
            assert!(before.contains_key(&cycle));
            assert!(
                !before
                    .keys()
                    .any(|path| path != &cycle && path.starts_with(&cycle))
            );
        }

        fs::write(assets.join("photo.jpg"), [3_u8, 2, 1]).expect("same-size asset edit");
        let after = scan_canvas_watched(&root);
        assert_ne!(
            before.get(&observed_root.join("fixtures/assets/photo.jpg")),
            after.get(&observed_root.join("fixtures/assets/photo.jpg"))
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn captured_instagram_snapshot_build_matches_the_disk_export_builder() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("examples/instagram-uhura");
        let common = CommonArgs {
            root: root.clone(),
            format_json: false,
            deny_warnings: false,
            emit_ir: false,
        };
        let snapshot = capture_canvas_snapshot(&root);

        let disk = crate::cmd::project::build(&common).expect("disk Canvas build");
        let captured =
            crate::cmd::project::build_snapshot(&snapshot.files).expect("snapshot Canvas build");

        assert_eq!(captured.html, disk.html);
        assert_eq!(captured.preview_count, disk.preview_count);
        assert_eq!(captured.replay_derived_count, disk.replay_derived_count);
        assert_eq!(captured.warnings, disk.warnings);
    }

    #[test]
    fn patching_our_export_does_not_swallow_a_source_save_during_activation() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-canvas-baseline-{}-{unique}",
            std::process::id()
        ));
        let source = root.join("app/page.uhura");
        let out = root.join("export");
        fs::create_dir_all(source.parent().expect("source parent")).expect("source directory");
        fs::write(&source, "page A").expect("source A");
        let observed_source = fs::canonicalize(&source).expect("canonical source");
        let mut built = scan_canvas_watched(&root);
        let built_source = built
            .get(&observed_source)
            .cloned()
            .expect("source fingerprint");

        // This save lands after the candidate's stable scan. The host also
        // writes its optional export before updating the watcher baseline.
        fs::write(&source, "page B").expect("source B");
        export_and_patch(&root, &out, b"Canvas A", &mut built).expect("Canvas export");

        assert_eq!(built.get(&observed_source), Some(&built_source));
        assert!(built.contains_key(&fs::canonicalize(out.join("canvas.html")).expect("export")));
        assert_ne!(built, scan_canvas_watched(&root));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn declared_export_under_generated_root_is_patched_without_self_triggering() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-declared-canvas-output-{}-{unique}",
            std::process::id()
        ));
        let out = root.join("build");
        fs::create_dir_all(&out).expect("output directory");
        fs::write(
            root.join("uhura.toml"),
            "[app]\nname = \"test-app\"\nentry = \"home\"\n\n[catalog]\npath = \"build/canvas.html\"\n",
        )
        .expect("manifest");
        fs::write(out.join("canvas.html"), "catalog before export").expect("declared output");
        let mut baseline = scan_canvas_watched(&root);
        let output = fs::canonicalize(out.join("canvas.html")).expect("canonical output");
        assert!(baseline.contains_key(&output));

        export_and_patch(&root, &out, b"generated Canvas", &mut baseline).expect("export write");

        assert_eq!(baseline, scan_canvas_watched(&root));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_exports_patch_every_output_alias_without_swallowing_source_saves() {
        use std::os::unix::fs::symlink;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-symlinked-canvas-output-{}-{unique}",
            std::process::id()
        ));
        let source = root.join("app/page.uhura");
        let target_out = root.join("export-target");
        let linked_out = root.join("export-link");
        fs::create_dir_all(source.parent().expect("source parent")).expect("source directory");
        fs::create_dir_all(&target_out).expect("target output directory");
        fs::write(&source, "page A").expect("source A");
        symlink("export-target", &linked_out).expect("output-directory symlink");

        let observed_root = fs::canonicalize(&root).expect("canonical root");
        let observed_source = observed_root.join("app/page.uhura");
        let linked_output = observed_root.join("export-link/canvas.html");
        let target_output = observed_root.join("export-target/canvas.html");
        let mut baseline = scan_canvas_watched(&root);

        export_and_patch(
            &root,
            &linked_out,
            b"Canvas through linked directory",
            &mut baseline,
        )
        .expect("linked-directory export");

        assert_eq!(baseline, scan_canvas_watched(&root));
        assert_eq!(baseline.get(&linked_output), baseline.get(&target_output));

        let source_before = baseline
            .get(&observed_source)
            .cloned()
            .expect("source fingerprint");
        fs::write(&source, "page B").expect("concurrent source save");
        export_and_patch(
            &root,
            &linked_out,
            b"Canvas after source save",
            &mut baseline,
        )
        .expect("second linked-directory export");
        let current = scan_canvas_watched(&root);

        assert_eq!(baseline.get(&observed_source), Some(&source_before));
        assert_eq!(baseline.get(&linked_output), current.get(&linked_output));
        assert_eq!(baseline.get(&target_output), current.get(&target_output));
        assert_ne!(baseline, current);

        let file_link_out = root.join("file-link-export");
        fs::create_dir_all(&file_link_out).expect("file-link output directory");
        symlink("../canvas-target.html", file_link_out.join("canvas.html"))
            .expect("dangling output-file symlink");
        let mut file_link_baseline = scan_canvas_watched(&root);

        export_and_patch(
            &root,
            &file_link_out,
            b"Canvas through file link",
            &mut file_link_baseline,
        )
        .expect("linked-file export");

        let current = scan_canvas_watched(&root);
        let logical_file_link = observed_root.join("file-link-export/canvas.html");
        assert_eq!(file_link_baseline, current);
        assert!(
            file_link_baseline
                .get(&logical_file_link)
                .is_some_and(|identity| identity.starts_with("!symlink-file:"))
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn output_patch_does_not_absorb_retargeted_or_new_aliases() {
        use std::os::unix::fs::symlink;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-canvas-output-race-{}-{unique}",
            std::process::id()
        ));
        let target_a = root.join("target-a");
        let target_b = root.join("target-b");
        let linked_out = root.join("out-link");
        fs::create_dir_all(&target_a).expect("target A");
        fs::create_dir_all(&target_b).expect("target B");
        fs::write(target_a.join("canvas.html"), "Canvas A before").expect("Canvas A");
        fs::write(target_b.join("canvas.html"), "Canvas B before").expect("Canvas B");
        symlink("target-a", &linked_out).expect("initial output link");

        let mut baseline = scan_canvas_watched(&root);
        let destination = linked_out.join("canvas.html");
        let patch = CanvasOutputBaselinePatch::capture(&root, &destination, &baseline)
            .expect("pre-write patch token");

        fs::remove_file(&linked_out).expect("remove initial output link");
        symlink("target-b", &linked_out).expect("retarget output link");
        write_canvas_output(&destination, b"Canvas host write", false).expect("host write");
        patch.apply(&root, b"Canvas host write", &mut baseline);

        let current = scan_canvas_watched(&root);
        let observed_root = fs::canonicalize(&root).expect("canonical root");
        let target_b_output = observed_root.join("target-b/canvas.html");
        assert_ne!(
            baseline.get(&target_b_output),
            current.get(&target_b_output)
        );
        assert_ne!(baseline, current);

        // A new alias created after authorization is user-observed state, not
        // something the host may import into the accepted baseline.
        fs::remove_file(&linked_out).expect("remove retargeted link");
        symlink("target-a", &linked_out).expect("restore output link");
        let mut baseline = scan_canvas_watched(&root);
        let patch = CanvasOutputBaselinePatch::capture(&root, &destination, &baseline)
            .expect("second pre-write patch token");
        let new_alias = root.join("new-canvas-alias.html");
        symlink("target-a/canvas.html", &new_alias).expect("concurrent new alias");
        write_canvas_output(&destination, b"Canvas second host write", false)
            .expect("second host write");
        patch.apply(&root, b"Canvas second host write", &mut baseline);

        let current = scan_canvas_watched(&root);
        let observed_new_alias = observed_root.join("new-canvas-alias.html");
        assert!(!baseline.contains_key(&observed_new_alias));
        assert!(current.contains_key(&observed_new_alias));
        assert_ne!(baseline, current);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rejected_canvas_does_not_patch_a_concurrent_output_repair() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-rejected-canvas-output-{}-{unique}",
            std::process::id()
        ));
        let out = root.join("export");
        fs::create_dir_all(&out).expect("output directory");
        fs::write(out.join("canvas.html"), "broken dependency").expect("initial output");
        let mut baseline = scan_canvas_watched(&root);
        let accepted_baseline = baseline.clone();
        fs::write(out.join("canvas.html"), "repaired dependency").expect("output repair");

        let common = CommonArgs {
            root: root.clone(),
            format_json: false,
            deny_warnings: false,
            emit_ir: false,
        };
        let failure = match crate::cmd::project::build(&common) {
            Ok(_) => panic!("missing project sources must reject"),
            Err(failure) => failure,
        };
        let state = RwLock::new(DevState {
            generation: 0,
            ok: false,
            diagnostics: None,
            good: None,
            canvas: Some(CanvasState::default()),
        });
        let clients: super::Clients = Arc::new(Mutex::new(Vec::new()));

        settle_canvas(
            Err(failure),
            Some(&out),
            &root,
            &mut baseline,
            &state,
            &clients,
        );

        assert_eq!(baseline, accepted_baseline);
        assert_ne!(baseline, scan_canvas_watched(&root));
        assert_eq!(
            fs::read_to_string(out.join("canvas.html")).expect("repaired output"),
            "repaired dependency"
        );
        let state = state.read().expect("state");
        let canvas = state.canvas.as_ref().expect("Canvas");
        assert_eq!(canvas.candidate_generation, 1);
        assert!(canvas.active_generation.is_none());
        assert!(canvas.diagnostics.is_some());
        drop(state);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn export_failure_does_not_reject_a_valid_in_memory_canvas() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-canvas-export-failure-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("test directory");
        let blocked_out = root.join("not-a-directory");
        fs::write(&blocked_out, "file blocks create_dir_all").expect("output blocker");
        let state = RwLock::new(DevState {
            generation: 0,
            ok: false,
            diagnostics: None,
            good: None,
            canvas: Some(CanvasState::default()),
        });
        let clients: super::Clients = Arc::new(Mutex::new(Vec::new()));
        let mut baseline = scan_canvas_watched(&root);
        let accepted_baseline = baseline.clone();
        fs::write(&blocked_out, "concurrent blocker repair").expect("repair blocker contents");

        settle_canvas(
            Ok(crate::cmd::project::CanvasArtifact {
                html: "Canvas in memory".to_string(),
                preview_count: 1,
                replay_derived_count: 0,
                warnings: None,
            }),
            Some(&blocked_out),
            &root,
            &mut baseline,
            &state,
            &clients,
        );

        let state = state.read().expect("state");
        let canvas = state.canvas.as_ref().expect("Canvas");
        assert_eq!(canvas.good_html.as_deref(), Some("Canvas in memory"));
        assert_eq!(canvas.active_generation, Some(1));
        assert!(canvas.active_build_id.is_some());
        assert!(canvas.diagnostics.is_none());
        drop(state);
        assert_eq!(baseline, accepted_baseline);
        assert_ne!(baseline, scan_canvas_watched(&root));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
