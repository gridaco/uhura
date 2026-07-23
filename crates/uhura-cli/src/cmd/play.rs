//! Standalone `uhura play` transport and observer adapter.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use uhura_host::{
    Host, ProjectSourceFingerprint, ProjectSourceSnapshot, RequestMethod, RouteRequest, WebAssets,
    build_candidate, capture_project_snapshot,
};

use crate::CommonArgs;

const REQUEST_WORKERS: usize = 8;
const REQUEST_QUEUE_CAPACITY: usize = 64;
const SSE_SESSION_LIMIT: usize = 4;

struct BoundedExecutor<T> {
    sender: mpsc::SyncSender<T>,
}

impl<T> BoundedExecutor<T>
where
    T: Send + 'static,
{
    fn new<F>(
        thread_name: &str,
        worker_count: usize,
        queue_capacity: usize,
        handler: F,
    ) -> std::io::Result<Self>
    where
        F: Fn(T) + Send + Sync + 'static,
    {
        assert!(worker_count > 0, "a bounded executor needs a worker");
        let (sender, receiver) = mpsc::sync_channel::<T>(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let handler = Arc::new(handler);

        for worker in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            let handler = Arc::clone(&handler);
            let worker_name = format!("{thread_name}-{worker}");
            drop(
                std::thread::Builder::new()
                    .name(worker_name)
                    .spawn(move || {
                        loop {
                            let task = receiver
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .recv();
                            match task {
                                Ok(task) => handler(task),
                                Err(_) => break,
                            }
                        }
                    })?,
            );
        }

        Ok(Self { sender })
    }

    fn try_submit(&self, task: T) -> Result<(), mpsc::TrySendError<T>> {
        self.sender.try_send(task)
    }
}

struct AdmissionLimit {
    limit: usize,
    admitted: AtomicUsize,
}

impl AdmissionLimit {
    fn new(limit: usize) -> Arc<Self> {
        assert!(limit > 0, "an admission limit must allow one session");
        Arc::new(Self {
            limit,
            admitted: AtomicUsize::new(0),
        })
    }

    fn try_acquire(self: &Arc<Self>) -> Option<AdmissionPermit> {
        self.admitted
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |admitted| {
                (admitted < self.limit).then_some(admitted + 1)
            })
            .ok()?;
        Some(AdmissionPermit {
            limit: Arc::clone(self),
        })
    }
}

struct AdmissionPermit {
    limit: Arc<AdmissionLimit>,
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        let previous = self.limit.admitted.fetch_sub(1, Ordering::Release);
        debug_assert!(previous > 0, "admission permits cannot underflow");
    }
}

struct RequestTask {
    request: tiny_http::Request,
    host: Arc<Host>,
    _sse_permit: Option<AdmissionPermit>,
}

struct RequestDispatcher {
    requests: BoundedExecutor<RequestTask>,
    sse: BoundedExecutor<RequestTask>,
    sse_limit: Arc<AdmissionLimit>,
}

impl RequestDispatcher {
    fn new() -> std::io::Result<Self> {
        // Event streams can live for the browser session, so they receive a
        // disjoint worker lane and admission limit. They can never consume the
        // workers that keep ordinary Editor and Play requests responsive.
        Ok(Self {
            requests: BoundedExecutor::new(
                "uhura-request",
                REQUEST_WORKERS,
                REQUEST_QUEUE_CAPACITY,
                execute_request,
            )?,
            sse: BoundedExecutor::new(
                "uhura-sse",
                SSE_SESSION_LIMIT,
                SSE_SESSION_LIMIT,
                execute_request,
            )?,
            sse_limit: AdmissionLimit::new(SSE_SESSION_LIMIT),
        })
    }

    fn dispatch(&self, request: tiny_http::Request, host: Arc<Host>) {
        if is_sse_request(&request) {
            let Some(permit) = self.sse_limit.try_acquire() else {
                reject_overloaded(request, "too many live Uhura event streams");
                return;
            };
            self.submit(
                &self.sse,
                RequestTask {
                    request,
                    host,
                    _sse_permit: Some(permit),
                },
                "Uhura event-stream workers are unavailable",
            );
        } else {
            self.submit(
                &self.requests,
                RequestTask {
                    request,
                    host,
                    _sse_permit: None,
                },
                "Uhura request workers are saturated",
            );
        }
    }

    fn submit(
        &self,
        executor: &BoundedExecutor<RequestTask>,
        task: RequestTask,
        message: &'static str,
    ) {
        if let Err(error) = executor.try_submit(task) {
            let task = match error {
                mpsc::TrySendError::Full(task) | mpsc::TrySendError::Disconnected(task) => task,
            };
            reject_overloaded(task.request, message);
        }
    }
}

fn execute_request(task: RequestTask) {
    respond(task.request, &task.host);
}

fn is_sse_request(request: &tiny_http::Request) -> bool {
    is_sse_route(request.method(), request.url())
}

fn is_sse_route(method: &tiny_http::Method, url: &str) -> bool {
    method == &tiny_http::Method::Get
        && matches!(
            url.split_once('?').map_or(url, |(path, _)| path),
            "/api/editor/events" | "/api/play/events"
        )
}

fn reject_overloaded(request: tiny_http::Request, message: &str) {
    let retry_after = tiny_http::Header::from_bytes("Retry-After", "1")
        .expect("static overload response header is valid");
    let response = tiny_http::Response::from_string(format!("{message}; retry shortly\n"))
        .with_status_code(tiny_http::StatusCode(503))
        .with_header(retry_after);
    let _ = request.respond(response);
}

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
    let dispatcher = match RequestDispatcher::new() {
        Ok(dispatcher) => dispatcher,
        Err(error) => {
            eprintln!("{command}: could not start bounded request workers: {error}");
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
        dispatcher.dispatch(request, Arc::clone(&host));
    }
    ExitCode::SUCCESS
}

pub(super) fn locate_web_assets() -> Result<WebAssets, String> {
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

pub(super) fn locate_export_web_assets() -> Result<WebAssets, String> {
    let mut candidates = Vec::new();
    if let Some(explicit) = std::env::var_os("UHURA_EXPORT_WEB_DIST") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(bin) = executable.parent()
    {
        candidates.push(bin.join("../share/uhura/web-export"));
    }
    candidates.push(tool_root().join("web/dist-export"));

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
        "export browser application is not built (looked for {locations}); set \
         UHURA_EXPORT_WEB_DIST or build the export Web profile into web/dist-export before \
         running `uhura export`"
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

pub(super) fn project_fingerprint(root: &Path) -> ProjectSourceFingerprint {
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

pub(super) fn build_stable_candidate(
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
    )
    // Byte responses already carry their exact representation length from
    // `uhura-host`. Keep that metadata on the wire even for larger Editor
    // snapshots; unknown-length SSE bodies remain chunked automatically.
    .with_chunked_threshold(usize::MAX);
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    struct TestTask {
        id: usize,
        release: mpsc::Receiver<()>,
    }

    #[test]
    fn bounded_executor_rejects_saturation_and_runs_queued_work_after_release() {
        let (started_tx, started_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let executor = BoundedExecutor::new("uhura-test-worker", 1, 1, move |task: TestTask| {
            started_tx.send(task.id).unwrap();
            task.release.recv().unwrap();
            finished_tx.send(task.id).unwrap();
        })
        .unwrap();

        let (release_first, first) = mpsc::channel();
        executor
            .try_submit(TestTask {
                id: 1,
                release: first,
            })
            .unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(1)), Ok(1));

        let (release_second, second) = mpsc::channel();
        executor
            .try_submit(TestTask {
                id: 2,
                release: second,
            })
            .unwrap();
        let (_release_rejected, rejected) = mpsc::channel();
        assert!(matches!(
            executor.try_submit(TestTask {
                id: 3,
                release: rejected,
            }),
            Err(mpsc::TrySendError::Full(TestTask { id: 3, .. }))
        ));

        release_first.send(()).unwrap();
        assert_eq!(finished_rx.recv_timeout(Duration::from_secs(1)), Ok(1));
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(1)), Ok(2));
        release_second.send(()).unwrap();
        assert_eq!(finished_rx.recv_timeout(Duration::from_secs(1)), Ok(2));
    }

    #[test]
    fn admission_limit_caps_sessions_and_releases_permits() {
        let limit = AdmissionLimit::new(2);
        let first = limit.try_acquire().expect("first session");
        let second = limit.try_acquire().expect("second session");
        assert!(limit.try_acquire().is_none(), "third session exceeds cap");

        drop(first);
        let replacement = limit.try_acquire().expect("released slot is reusable");
        assert!(limit.try_acquire().is_none(), "cap remains exact");

        drop(second);
        drop(replacement);
        assert_eq!(limit.admitted.load(Ordering::Acquire), 0);
    }

    #[test]
    fn only_get_event_endpoints_enter_the_sse_lane() {
        assert!(is_sse_route(&tiny_http::Method::Get, "/api/editor/events"));
        assert!(is_sse_route(
            &tiny_http::Method::Get,
            "/api/play/events?generation=2"
        ));
        assert!(!is_sse_route(
            &tiny_http::Method::Head,
            "/api/editor/events"
        ));
        assert!(!is_sse_route(&tiny_http::Method::Get, "/api/editor/state"));
    }
}
