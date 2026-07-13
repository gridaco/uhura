//! `uhura dev [path] [--port <n>]` — the play server (design §12.4):
//! tiny_http + SSE. Watch `*.{uhura,toml,css}` under the corpus (100 ms
//! debounce) → recheck natively → serve the LAST-GOOD IR + compiled
//! stylesheet from memory; a failing check pushes its diagnostics
//! envelope over `/events` and the shell overlays it OVER the running
//! app. Reload policy is full restart — state-preserving reload is an
//! open RFC topic the spike must not fake.
//!
//! Endpoints: `/` `/shell/*` `/wasm/*` `/ir.json` `/stylesheet.css`
//! `/fixture.json` `/script.json` `/boot.json` `/icons.json` `/assets/*`
//! `/events` (SSE). Nothing is ever written into the watched tree.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use uhura_base::{Severity, to_canonical_json, to_envelope};
use uhura_check::check;
use uhura_check::fixture::load_fixture;
use uhura_core::ir::ProgramIr;

use crate::CommonArgs;
use crate::cmd::trace::{boot_updates, fixture_slices_json};

pub fn run(common: &CommonArgs, port: u16) -> ExitCode {
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
            (Some(_), true) => println!("uhura dev: checked clean"),
            (Some(_), false) => println!("uhura dev: check FAILING — serving the last good build"),
            (None, _) => println!("uhura dev: check FAILING — no good build yet (overlay only)"),
        }
    }

    let server = match tiny_http::Server::http(("127.0.0.1", port)) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("uhura dev: could not bind 127.0.0.1:{port}: {e}");
            return ExitCode::from(2);
        }
    };
    println!("uhura dev: http://127.0.0.1:{port}/ (tick=250ms, override with ?tick=<ms>)");

    // ── the watcher: poll mtimes, debounce, recheck, broadcast ──────────
    {
        let root = root.clone();
        let state = Arc::clone(&state);
        let clients = Arc::clone(&clients);
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
                    "uhura dev: generation {} — {}",
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
        std::thread::spawn(move || handle(request, &root, &state, &clients));
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
                "rule": "dev/recheck",
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

    Ok(GoodBuild {
        ir: program.to_canonical_string(),
        stylesheet: output.stylesheet.clone(),
        fixture_json,
        script_json: script_canonical,
        boot_json: boot_envelope(program, &fixture).map_err(fail)?,
        icons_json: icons_json(root, &manifest.catalog_path),
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

/// (path → (mtime, len)) for every watched file: `*.{uhura,toml,css}`,
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
            if !matches!(name.as_ref(), "build" | "renders" | "target" | ".git") {
                scan_dir(&path, out);
            }
            continue;
        }
        let watched = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("uhura" | "toml" | "css")
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
/// TOOLCHAIN tree, not the corpus (compile-time anchored; `uhura dev`
/// is a dev tool run from this repo).
fn tool_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn handle(request: tiny_http::Request, root: &Path, state: &RwLock<DevState>, clients: &Clients) {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");

    if path == "/events" {
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

    let outcome: Served = match path {
        "/" => file_bytes(&tool_root().join("shell/index.html")).map(|b| (ct("html"), b, None)),
        "/ir.json" => artifact(state, |g| g.ir.clone().into_bytes(), "json"),
        "/stylesheet.css" => artifact(state, |g| g.stylesheet.clone().into_bytes(), "css"),
        "/fixture.json" => artifact(state, |g| g.fixture_json.clone().into_bytes(), "json"),
        "/script.json" => artifact(state, |g| g.script_json.clone().into_bytes(), "json"),
        "/boot.json" => artifact(state, |g| g.boot_json.clone().into_bytes(), "json"),
        "/icons.json" => artifact(state, |g| g.icons_json.clone().into_bytes(), "json"),
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
    };

    let _ = match outcome {
        Ok((content_type, bytes, generation)) => {
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
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn header(name: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}
