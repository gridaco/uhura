//! Standalone `uhura play`/`uhura dev` transport and observer adapter.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use uhura_host::{
    Host, ProjectSourceFingerprint, ProjectSourceSnapshot, RequestMethod, RouteRequest, WebAssets,
    build_candidate, capture_project_snapshot,
};

use crate::CommonArgs;

pub use uhura_host::boot_envelope;

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
            Self::Editor => "uhura editor",
            Self::Play => "uhura play",
        }
    }

    fn route(self) -> &'static str {
        match self {
            Self::Editor => "/",
            Self::Play => "/play",
        }
    }
}

fn run_host(common: &CommonArgs, port: u16, primary: PrimarySurface) -> ExitCode {
    let command = primary.command();
    let web = match locate_web_assets() {
        Ok(web) => web,
        Err(error) => {
            eprintln!("{command}: {error}");
            return ExitCode::from(2);
        }
    };

    let root = common.root.clone();
    let first_observation = project_fingerprint(&root);
    let (candidate, baseline_snapshot) = build_stable_candidate(&root, first_observation, 1);
    let baseline = baseline_snapshot.fingerprint().clone();
    let (host, report) = match Host::new(web, candidate) {
        Ok(host) => host,
        Err(error) => {
            eprintln!("{command}: could not publish initial host state: {error}");
            return ExitCode::from(2);
        }
    };
    print_initial_report(command, report);

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

    let host = Arc::new(host);
    {
        let root = root.clone();
        let host = Arc::clone(&host);
        std::thread::spawn(move || observe(root, host, baseline));
    }

    let server = Arc::new(server);
    for request in server.incoming_requests() {
        let host = Arc::clone(&host);
        std::thread::spawn(move || respond(request, &host));
    }
    ExitCode::SUCCESS
}

fn locate_web_assets() -> Result<WebAssets, String> {
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

    let mut attempted = Vec::new();
    for root in candidates {
        if attempted.contains(&root) {
            continue;
        }
        attempted.push(root.clone());
        let index_path = root.join("index.html");
        match std::fs::symlink_metadata(&index_path) {
            Ok(_) => {
                let wasm_root = locate_wasm_for(&root);
                return if wasm_root.is_dir() {
                    WebAssets::from_directories(&root, &wasm_root)
                } else {
                    WebAssets::from_frontend_directory(&root)
                };
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

fn print_initial_report(command: &str, report: uhura_host::PublicationReport) {
    if report.editor_current {
        println!(
            "{command}: Editor revision 1 — {} previews ({} replay-derived)",
            report.preview_count.unwrap_or(0),
            report.replay_derived_count.unwrap_or(0)
        );
    } else {
        println!("{command}: Editor revision 1 rejected — application starts with diagnostics");
    }
    if report.play_ok {
        println!("{command}: Play checked clean");
    } else if report.has_good_play {
        println!("{command}: Play check failing — serving the last good build");
    } else {
        println!("{command}: Play check failing — no good build yet");
    }
}

fn observe(root: std::path::PathBuf, host: Arc<Host>, mut seen: ProjectSourceFingerprint) {
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

        let revision = host.source_revision() + 1;
        let (candidate, settled) = build_stable_candidate(&root, stable, revision);
        seen = settled.fingerprint().clone();
        match host.publish(candidate) {
            Err(error) => {
                eprintln!("uhura host: could not publish revision {revision}: {error}");
            }
            Ok(report) => {
                if report.editor_current {
                    println!(
                        "uhura host: Editor revision {revision} current — {} previews \
                         ({} replay-derived)",
                        report.preview_count.unwrap_or(0),
                        report.replay_derived_count.unwrap_or(0)
                    );
                } else {
                    println!(
                        "uhura host: Editor revision {revision} rejected — last render is stale"
                    );
                }
                println!(
                    "uhura host: Play generation {} — {}",
                    report.play_generation,
                    if report.play_ok {
                        "ok, clients reload"
                    } else {
                        "check failing, last-good runtime retained"
                    }
                );
            }
        }
    }
}

fn project_fingerprint(root: &Path) -> ProjectSourceFingerprint {
    capture_project_snapshot(root).fingerprint().clone()
}

fn wait_for_stable_fingerprint(
    root: &Path,
    mut observed: ProjectSourceFingerprint,
) -> ProjectSourceFingerprint {
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let again = project_fingerprint(root);
        if again == observed {
            return observed;
        }
        observed = again;
    }
}

fn build_stable_candidate(
    root: &Path,
    mut before: ProjectSourceFingerprint,
    revision: u64,
) -> (uhura_host::ClientCandidate, ProjectSourceSnapshot) {
    loop {
        let snapshot = capture_project_snapshot(root);
        if before != *snapshot.fingerprint() {
            before = wait_for_stable_fingerprint(root, snapshot.fingerprint().clone());
            continue;
        }

        let candidate = build_candidate(&snapshot, revision);
        let after = project_fingerprint(root);
        if snapshot.fingerprint() == &after {
            return (candidate, snapshot);
        }
        before = wait_for_stable_fingerprint(root, after);
    }
}

fn respond(request: tiny_http::Request, host: &Host) {
    let method = match request.method() {
        tiny_http::Method::Get => RequestMethod::Get,
        tiny_http::Method::Head => RequestMethod::Head,
        _ => RequestMethod::Other,
    };
    let response = host.route(RouteRequest {
        method,
        url: request.url(),
    });
    let headers = response
        .headers
        .iter()
        .map(|(name, value)| {
            tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes())
                .expect("host response headers are valid")
        })
        .collect();
    let length = response.body.content_length();
    let response = tiny_http::Response::new(
        tiny_http::StatusCode(response.status),
        headers,
        response.body,
        length,
        None,
    );
    let _ = request.respond(response);
}
