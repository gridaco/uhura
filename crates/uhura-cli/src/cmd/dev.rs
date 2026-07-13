//! `uhura play [path] [--port <n>]` — the interactive play server (design
//! §12.4). `uhura dev` remains a compatibility alias.
//! tiny_http + SSE. Watch `*.{uhura,toml,css,js}` under the corpus (100 ms
//! debounce) → recheck natively → serve the LAST-GOOD IR + compiled
//! stylesheet from memory; a failing check pushes its diagnostics
//! envelope over `/events` and the shell overlays it OVER the running
//! app. Reload policy is full restart — state-preserving reload is an
//! open RFC topic the spike must not fake.
//!
//! Endpoints: `/` `/shell/*` `/wasm/*` `/ir.json` `/stylesheet.css`
//! `/fixture.json` `/script.json` `/boot.json` `/icons.json` `/play.json`
//! `/provider.js` `/assets/*` `/events` (SSE). Nothing is ever written into
//! the watched tree.

use std::collections::BTreeMap;
use std::io::Read;
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
    run_host(common, port, HostEntry::Play)
}

/// Host the read-only Canvas and the live Play runtime on one origin. Keeping
/// this in the Play server means `/play` exercises the exact same artifacts,
/// provider, watcher, and SSE path as the dedicated command.
pub(crate) fn run_with_editor(common: &CommonArgs, port: u16, canvas: Vec<u8>) -> ExitCode {
    run_host(common, port, HostEntry::Editor(Arc::from(canvas)))
}

#[derive(Clone)]
enum HostEntry {
    Play,
    Editor(Arc<[u8]>),
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
            Self::Editor(_) => "uhura editor",
        }
    }

    fn document(&self, path: &str) -> Option<EntryDocument> {
        match self {
            Self::Play if path == "/" => Some(EntryDocument::Play),
            Self::Editor(_) => match path {
                "/" | "/index.html" | "/canvas.html" => Some(EntryDocument::Canvas),
                "/play" | "/play/" => Some(EntryDocument::Play),
                "/favicon.ico" => Some(EntryDocument::EmptyFavicon),
                _ => None,
            },
            _ => None,
        }
    }
}

fn run_host(common: &CommonArgs, port: u16, entry: HostEntry) -> ExitCode {
    let command = entry.command();
    let root = common.root.clone();
    let state = Arc::new(RwLock::new(DevState {
        generation: 0,
        ok: false,
        diagnostics: None,
        good: None,
    }));
    let clients: Clients = Arc::new(Mutex::new(Vec::new()));

    // ── first check, synchronously: the URL we print is honest ──────────
    // The watch baseline is scanned FIRST so an edit landing during the
    // initial check is still detected as a change.
    let baseline = scan_watched(&root);
    recheck_into(&root, &state, &clients);
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
        HostEntry::Editor(_) => {
            println!("{command}: http://127.0.0.1:{port}/ (read-only; restart to rebuild Canvas)");
            println!("{command}: Play http://127.0.0.1:{port}/play");
        }
    }

    // ── the watcher: poll mtimes, debounce, recheck, broadcast ──────────
    {
        let root = root.clone();
        let state = Arc::clone(&state);
        let clients = Arc::clone(&clients);
        let command = entry.command();
        std::thread::spawn(move || {
            let mut seen = baseline;
            loop {
                std::thread::sleep(Duration::from_millis(150));
                let now = scan_watched(&root);
                if now == seen {
                    continue;
                }
                // Debounce: wait for the tree to sit still for 100 ms.
                let mut stable = now;
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    let again = scan_watched(&root);
                    if again == stable {
                        break;
                    }
                    stable = again;
                }
                seen = stable;
                recheck_into(&root, &state, &clients);
                let s = state.read().expect("state lock");
                println!(
                    "{command}: generation {} — {}",
                    s.generation,
                    if s.ok {
                        "ok, shell reloads"
                    } else {
                        "check failing, overlay pushed"
                    }
                );
            }
        });
    }

    // ── serve: one thread per request (SSE holds its thread) ────────────
    let server = Arc::new(server);
    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        let clients = Arc::clone(&clients);
        let root = root.clone();
        let entry = entry.clone();
        std::thread::spawn(move || handle(request, &root, &state, &clients, &entry));
    }
    ExitCode::SUCCESS
}

// ── state ───────────────────────────────────────────────────────────────────

struct DevState {
    generation: u64,
    ok: bool,
    /// `uhura-diagnostics/0` envelope of the latest FAILING check.
    diagnostics: Option<serde_json::Value>,
    /// Last-good artifacts — what every endpoint serves (§12.4).
    good: Option<GoodBuild>,
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

type Clients = Arc<Mutex<Vec<Sender<String>>>>;

/// (content-type, body, generation stamp) or (status, message).
type Served = Result<(String, Vec<u8>, Option<u64>), (u16, String)>;

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

/// (path → (mtime, len)) for every watched file: `*.{uhura,toml,css,js}`,
/// excluding emitted/derived trees and the lock (check writes it when
/// absent — watching it would loop).
fn scan_watched(root: &Path) -> BTreeMap<PathBuf, (SystemTime, u64)> {
    let mut out = BTreeMap::new();
    scan_dir(root, &mut out);
    out
}

fn scan_dir(dir: &Path, out: &mut BTreeMap<PathBuf, (SystemTime, u64)>) {
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
                scan_dir(&path, out);
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

/// The uhura workspace root — the shell and wasm bundle live in the
/// TOOLCHAIN tree, not the corpus (compile-time anchored; `uhura play`
/// is a dev tool run from this repo).
fn tool_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn handle(
    request: tiny_http::Request,
    root: &Path,
    state: &RwLock<DevState>,
    clients: &Clients,
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
            let mut clients = clients.lock().expect("clients lock");
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

    let outcome: Served = match entry.document(path) {
        Some(EntryDocument::Canvas) => match entry {
            HostEntry::Editor(canvas) => Ok((ct("html"), canvas.to_vec(), None)),
            HostEntry::Play => unreachable!("Play has no Canvas route"),
        },
        Some(EntryDocument::Play) => play_document(matches!(entry, HostEntry::Editor(_))),
        Some(EntryDocument::EmptyFavicon) => Ok((ct("ico"), Vec::new(), None)),
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
                    serve_tree(&tool_root().join("shell"), rel)
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

fn split_request_url(url: &str) -> (&str, Option<&str>) {
    url.split_once('?')
        .map_or((url, None), |(path, query)| (path, Some(query)))
}

/// The editor's `/play` document differs from dedicated Play by one host-owned
/// return link. The runtime scripts and every endpoint remain identical.
fn play_document(with_editor_navigation: bool) -> Served {
    let bytes = file_bytes(&tool_root().join("shell/index.html"))?;
    if !with_editor_navigation {
        return Ok((ct("html"), bytes, None));
    }
    let shell = String::from_utf8(bytes)
        .map_err(|error| (500, format!("shell/index.html is not UTF-8: {error}")))?;
    Ok((
        ct("html"),
        super::editor::play_html(&shell).into_bytes(),
        None,
    ))
}

/// A last-good in-memory artifact; 503 + the failure story before the
/// first clean check. The generation stamp lets the shell detect a
/// recheck landing between its artifact fetches (mixed-build boot).
fn artifact(state: &RwLock<DevState>, pick: impl Fn(&GoodBuild) -> Vec<u8>, ext: &str) -> Served {
    let s = state.read().expect("state lock");
    match &s.good {
        Some(good) => Ok((ct(ext), pick(good), Some(s.generation))),
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
                Ok((ct("js"), module.clone().into_bytes(), Some(s.generation)))
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
    file_bytes(&path).map(|b| (ct(ext), b, None))
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
    use std::sync::Arc;

    use super::{EntryDocument, HostEntry, ct, play_document, split_request_url};

    #[test]
    fn editor_and_dedicated_play_have_distinct_entry_routes() {
        let editor = HostEntry::Editor(Arc::from(Vec::<u8>::new()));
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
        let (_, dedicated, _) = play_document(false).expect("dedicated Play document");
        let (_, editor, _) = play_document(true).expect("editor-hosted Play document");
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
}
