//! `uhura project [path] [--out=<dir>]` — checked program → resolved
//! example previews (pinned + replay-derived, §6.2) → `eval_view` → V →
//! HTML → one self-contained `renders/canvas.html` (§8.3). Zero
//! transitions, commands, network — replay already happened in the check.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use uhura_base::{Severity, render_text, to_envelope};
use uhura_check::manifest::load_manifest;
use uhura_check::preview::{
    PreviewDataKind, PreviewDataValue, PreviewOrigin, PreviewPayload, PreviewSource,
};
use uhura_check::resolve::SubjectKind;
use uhura_check::{CheckInput, SourceInput, check};
use uhura_core::eval::{eval_fragment, eval_view};
use uhura_project::{
    Asset, FrameContent, FrameKind, PreviewField, PreviewFieldGroup, PreviewFieldValue,
    PreviewFrame, render_canvas,
};
use uhura_syntax::SourceKind;

use crate::CommonArgs;

/// Canonical corpus-relative file bytes captured by the Canvas observer.
///
/// References are normalized inside this immutable keyspace. Case aliases are
/// honored only when the capture established that the backing filesystem is
/// case-insensitive, preserving disk lookup semantics without making Linux
/// projects unexpectedly case-insensitive.
#[derive(Clone, Debug, Default)]
pub(crate) struct CanvasSourceFiles {
    entries: BTreeMap<PathBuf, Arc<[u8]>>,
    case_insensitive: bool,
}

impl CanvasSourceFiles {
    fn insert(&mut self, path: PathBuf, bytes: Arc<[u8]>) {
        self.entries.insert(path, bytes);
    }

    fn iter(&self) -> impl Iterator<Item = (&PathBuf, &Arc<[u8]>)> {
        self.entries.iter()
    }

    fn resolve(&self, path: &Path) -> Result<Option<&Arc<[u8]>>, String> {
        let path = normalize_corpus_path(path)?;
        if let Some(bytes) = self.entries.get(&path) {
            return Ok(Some(bytes));
        }
        if !self.case_insensitive {
            return Ok(None);
        }

        let Some(wanted) = case_key(&path) else {
            return Ok(None);
        };
        let mut found = None;
        for (candidate, bytes) in &self.entries {
            if case_key(candidate).as_ref() != Some(&wanted) {
                continue;
            }
            if found.is_some() {
                return Err(format!(
                    "{} is ambiguous in the captured project snapshot",
                    path.display()
                ));
            }
            found = Some(bytes);
        }
        Ok(found)
    }
}

fn normalize_corpus_path(path: &Path) -> Result<PathBuf, String> {
    let display = path.display().to_string();
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return Err(format!("{display}: path escapes the captured project root"));
                }
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(format!(
                    "{display}: expected a path relative to the captured project root"
                ));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(format!("{display}: expected a file path"));
    }
    Ok(normalized)
}

fn case_key(path: &Path) -> Option<Vec<String>> {
    path.components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .map(|part| part.to_lowercase())
        })
        .collect()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CanvasSourceFingerprint {
    entries: BTreeMap<PathBuf, String>,
    case_insensitive: bool,
}

impl Deref for CanvasSourceFingerprint {
    type Target = BTreeMap<PathBuf, String>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for CanvasSourceFingerprint {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CanvasCaptureFailure {
    path: PathBuf,
    operation: &'static str,
    message: String,
    blocks_canvas: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CanvasSourceSnapshot {
    pub(crate) files: CanvasSourceFiles,
    pub(crate) fingerprint: CanvasSourceFingerprint,
    failures: Vec<CanvasCaptureFailure>,
}

/// One completely checked, self-contained Canvas export held in memory.
///
/// Keeping the rendered document and its summary metadata together lets the
/// static export command and hosted Editor consume the same build product.
pub(crate) struct CanvasArtifact {
    pub(crate) html: String,
    pub(crate) preview_count: usize,
    pub(crate) replay_derived_count: usize,
    /// Standard diagnostics envelope for non-fatal warnings associated with
    /// this otherwise valid Canvas generation.
    pub(crate) warnings: Option<serde_json::Value>,
}

/// A rejected in-memory Canvas build. Hosts consume the diagnostics envelope;
/// command entry points additionally replay the legacy terminal report.
#[derive(Debug)]
pub(crate) struct CanvasBuildFailure {
    pub(crate) exit_code: ExitCode,
    pub(crate) envelope: serde_json::Value,
    terminal: TerminalFailure,
}

#[derive(Debug)]
enum TerminalFailure {
    Check(String),
    Message(String),
}

impl CanvasBuildFailure {
    fn check(diagnostics: &[uhura_base::Diagnostic], source_map: &uhura_base::SourceMap) -> Self {
        Self {
            exit_code: ExitCode::from(1),
            envelope: to_envelope(diagnostics, source_map),
            terminal: TerminalFailure::Check(render_text(diagnostics, source_map)),
        }
    }

    fn capture(failures: &[CanvasCaptureFailure]) -> Self {
        let mut details = failures
            .iter()
            .take(8)
            .map(|failure| {
                format!(
                    "{}: {}: {}",
                    failure.path.display(),
                    failure.operation,
                    failure.message
                )
            })
            .collect::<Vec<_>>();
        if failures.len() > details.len() {
            details.push(format!(
                "and {} more filesystem errors",
                failures.len() - details.len()
            ));
        }
        let detail = details.join("; ");
        let message = format!("could not capture a complete project snapshot: {detail}");
        Self::build(2, "canvas/input", message.clone(), message)
    }

    fn build(exit_code: u8, rule: &str, message: String, terminal: String) -> Self {
        Self::message(
            ExitCode::from(exit_code),
            rule,
            &message,
            TerminalFailure::Message(terminal),
        )
    }

    fn message(exit_code: ExitCode, rule: &str, message: &str, terminal: TerminalFailure) -> Self {
        Self {
            exit_code,
            envelope: failure_envelope(rule, message),
            terminal,
        }
    }

    fn report(self, command: &str) -> ExitCode {
        debug_assert_eq!(
            self.envelope
                .get("format")
                .and_then(serde_json::Value::as_str),
            Some("uhura-diagnostics")
        );
        match self.terminal {
            TerminalFailure::Check(rendered) => {
                print!("{rendered}");
                eprintln!("{command}: the check must come up clean first");
            }
            TerminalFailure::Message(message) => eprintln!("{command}: {message}"),
        }
        self.exit_code
    }
}

fn failure_envelope(rule: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "format": "uhura-diagnostics",
        "version": 0,
        "summary": { "errors": 1, "warnings": 0 },
        "diagnostics": [{
            "code": "UH9000",
            "rule": rule,
            "severity": "error",
            "message": message,
        }],
    })
}

pub fn run(common: &CommonArgs, out_dir: Option<&str>) -> ExitCode {
    run_as(common, out_dir, "uhura project")
}

/// Shared Canvas build path for the build-only compatibility command and the
/// read-only editor host. The command label keeps diagnostics honest about the
/// entry point the user actually invoked.
pub(crate) fn run_as(common: &CommonArgs, out_dir: Option<&str>, command: &str) -> ExitCode {
    let artifact = match build(common) {
        Ok(artifact) => artifact,
        Err(failure) => return failure.report(command),
    };

    let out_dir = out_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("renders"));
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("{command}: {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }
    let out_path = out_dir.join("canvas.html");
    if let Err(e) = std::fs::write(&out_path, &artifact.html) {
        eprintln!("{command}: {}: {e}", out_path.display());
        return ExitCode::from(2);
    }
    println!(
        "{command}: projected {} previews ({} replay-derived) → {} ({} KiB)",
        artifact.preview_count,
        artifact.replay_derived_count,
        out_path.display(),
        artifact.html.len() / 1024
    );
    ExitCode::SUCCESS
}

/// Check and project the corpus into the exact self-contained Canvas document
/// used by `uhura project`, without writing an output artifact to disk.
pub(crate) fn build(common: &CommonArgs) -> Result<CanvasArtifact, CanvasBuildFailure> {
    let snapshot = capture_canvas_snapshot(&common.root);
    build_captured_snapshot(&snapshot)
}

/// Build one already captured source revision. Both the one-shot exporter and
/// the live Editor use this exact entry point, including capture-error policy.
pub(crate) fn build_captured_snapshot(
    snapshot: &CanvasSourceSnapshot,
) -> Result<CanvasArtifact, CanvasBuildFailure> {
    let blocking = snapshot
        .failures
        .iter()
        .filter(|failure| failure.blocks_canvas)
        .cloned()
        .collect::<Vec<_>>();
    if !blocking.is_empty() {
        return Err(CanvasBuildFailure::capture(&blocking));
    }
    build_snapshot(&snapshot.files)
}

/// Capture every observable project input once. The bytes stored here are both
/// fingerprinted and consumed by the builder, so an attempt cannot mix file
/// revisions. Safe in-project symlinks retain logical identity; broad output
/// exclusions are overridden only for exact declared dependencies.
pub(crate) fn capture_canvas_snapshot(root: &Path) -> CanvasSourceSnapshot {
    capture_canvas_snapshot_with(root, &mut |path: &Path| std::fs::read(path))
}

fn capture_canvas_snapshot_with(
    root: &Path,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
) -> CanvasSourceSnapshot {
    let root = canvas_scan_root(root);
    let case_insensitive = detect_case_insensitive_filesystem(&root);
    let mut snapshot = CanvasSourceSnapshot::default();
    snapshot.files.case_insensitive = case_insensitive;
    snapshot.fingerprint.case_insensitive = case_insensitive;
    let mut ancestors = vec![root.clone()];
    capture_canvas_dir(
        &root,
        &root,
        &mut snapshot,
        read_file,
        case_insensitive,
        false,
        &mut ancestors,
    );
    capture_declared_dependencies(&root, &mut snapshot, read_file, case_insensitive);
    snapshot.failures.sort();
    snapshot
}

pub(crate) fn canvas_scan_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| {
        if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|current| current.join(root))
                .unwrap_or_else(|_| root.to_path_buf())
        }
    })
}

pub(crate) fn canvas_root_dir_is_generated(name: &str, case_insensitive: bool) -> bool {
    ["build", "renders", "target"]
        .into_iter()
        .any(|expected| canvas_name_matches(name, expected, case_insensitive))
        || canvas_dir_is_always_ignored(name, case_insensitive)
}

pub(crate) fn canvas_dir_is_always_ignored(name: &str, case_insensitive: bool) -> bool {
    ["node_modules", ".git"]
        .into_iter()
        .any(|expected| canvas_name_matches(name, expected, case_insensitive))
}

fn canvas_name_matches(actual: &str, expected: &str, case_insensitive: bool) -> bool {
    actual == expected || (case_insensitive && actual.eq_ignore_ascii_case(expected))
}

fn capture_canvas_dir(
    root: &Path,
    dir: &Path,
    snapshot: &mut CanvasSourceSnapshot,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    case_insensitive: bool,
    source_scope: bool,
    ancestors: &mut Vec<PathBuf>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            record_canvas_capture_failure(
                snapshot,
                dir,
                "read directory",
                error,
                source_scope || dir == root,
            );
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                record_canvas_capture_failure(
                    snapshot,
                    dir,
                    "read directory entry",
                    error,
                    source_scope || dir == root,
                );
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let child_source_scope = source_scope
            || (dir == root
                && ["app", "components", "surfaces"]
                    .into_iter()
                    .any(|expected| canvas_name_matches(&name, expected, case_insensitive)));
        let indeterminate_ignored_name = canvas_dir_is_always_ignored(&name, case_insensitive)
            || (dir == root && canvas_root_dir_is_generated(&name, case_insensitive));
        let blocks_canvas = canvas_path_blocks(root, &path, child_source_scope, case_insensitive);
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                record_canvas_capture_failure(
                    snapshot,
                    &path,
                    "inspect file type",
                    error,
                    !indeterminate_ignored_name
                        && canvas_indeterminate_path_blocks(
                            root,
                            &path,
                            child_source_scope,
                            case_insensitive,
                        ),
                );
                continue;
            }
        };
        if file_type.is_symlink() {
            capture_canvas_symlink(
                root,
                &path,
                snapshot,
                read_file,
                case_insensitive,
                child_source_scope,
                ancestors,
            );
            continue;
        }
        if file_type.is_dir() {
            let ignored = indeterminate_ignored_name;
            if !ignored {
                let canonical = match std::fs::canonicalize(&path) {
                    Ok(canonical) => canonical,
                    Err(error) => {
                        record_canvas_capture_failure(
                            snapshot,
                            &path,
                            "resolve directory",
                            error,
                            child_source_scope,
                        );
                        continue;
                    }
                };
                if !canonical.starts_with(root) {
                    record_canvas_capture_failure(
                        snapshot,
                        &path,
                        "keep directory inside project",
                        io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "directory escapes project",
                        ),
                        child_source_scope,
                    );
                    continue;
                }
                if ancestors.contains(&canonical) {
                    record_canvas_capture_failure(
                        snapshot,
                        &path,
                        "traverse directory",
                        io::Error::new(io::ErrorKind::InvalidData, "directory cycle"),
                        child_source_scope,
                    );
                    continue;
                }
                ancestors.push(canonical);
                capture_canvas_dir(
                    root,
                    &path,
                    snapshot,
                    read_file,
                    case_insensitive,
                    child_source_scope,
                    ancestors,
                );
                ancestors.pop();
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let bytes = match read_file(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                record_canvas_capture_failure(snapshot, &path, "read file", error, blocks_canvas);
                continue;
            }
        };
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative.to_path_buf(),
            Err(error) => {
                record_canvas_capture_failure(
                    snapshot,
                    &path,
                    "make path corpus-relative",
                    io::Error::new(io::ErrorKind::InvalidData, error.to_string()),
                    blocks_canvas,
                );
                continue;
            }
        };
        snapshot
            .fingerprint
            .insert(path, uhura_base::sha256_hex(&bytes));
        snapshot.files.insert(relative, Arc::from(bytes));
    }
}

fn canvas_path_blocks(
    root: &Path,
    path: &Path,
    source_scope: bool,
    case_insensitive: bool,
) -> bool {
    if source_scope
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".uhura"))
    {
        return true;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    path_components_equal(relative, Path::new("uhura.toml"), case_insensitive)
}

/// Before an entry's kind is known, any failure beneath a source root might
/// hide a source file or directory. Once a regular file is known, the narrower
/// extension/dependency policy in `canvas_path_blocks` applies instead.
fn canvas_indeterminate_path_blocks(
    root: &Path,
    path: &Path,
    source_scope: bool,
    case_insensitive: bool,
) -> bool {
    source_scope || canvas_path_blocks(root, path, source_scope, case_insensitive)
}

fn capture_canvas_symlink(
    root: &Path,
    path: &Path,
    snapshot: &mut CanvasSourceSnapshot,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    case_insensitive: bool,
    source_scope: bool,
    ancestors: &mut Vec<PathBuf>,
) {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let ignored_directory_name = canvas_dir_is_always_ignored(name, case_insensitive)
        || (path.parent() == Some(root) && canvas_root_dir_is_generated(name, case_insensitive));
    let path_blocks_canvas = canvas_path_blocks(root, path, source_scope, case_insensitive);
    let indeterminate_blocks_canvas = !ignored_directory_name
        && canvas_indeterminate_path_blocks(root, path, source_scope, case_insensitive);
    let link_target = match std::fs::read_link(path) {
        Ok(target) => target,
        Err(error) => {
            record_canvas_capture_failure(
                snapshot,
                path,
                "read symlink",
                error,
                indeterminate_blocks_canvas,
            );
            return;
        }
    };
    let target_identity = format!("{:?}", link_target.as_os_str());
    let canonical = match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(error) => {
            let error_identity = canvas_io_error_fingerprint("resolve symlink", &error);
            record_canvas_capture_failure(
                snapshot,
                path,
                "resolve symlink",
                error,
                indeterminate_blocks_canvas,
            );
            snapshot.fingerprint.insert(
                path.to_path_buf(),
                format!("!symlink-unresolved:{target_identity}:{error_identity}"),
            );
            return;
        }
    };
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            record_canvas_capture_failure(
                snapshot,
                path,
                "inspect symlink target",
                error,
                indeterminate_blocks_canvas,
            );
            return;
        }
    };
    if metadata.is_dir() && ignored_directory_name {
        return;
    }
    let blocks_canvas = path_blocks_canvas || (source_scope && metadata.is_dir());
    if !canonical.starts_with(root) {
        record_canvas_capture_failure(
            snapshot,
            path,
            "keep symlink inside project",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "symlink target escapes project",
            ),
            blocks_canvas,
        );
        return;
    }
    if metadata.is_dir() {
        snapshot.fingerprint.insert(
            path.to_path_buf(),
            format!("!symlink-directory:{target_identity}"),
        );
        if ancestors.contains(&canonical) {
            record_canvas_capture_failure(
                snapshot,
                path,
                "traverse symlink",
                io::Error::new(io::ErrorKind::InvalidData, "symlink directory cycle"),
                source_scope,
            );
            return;
        }
        ancestors.push(canonical);
        capture_canvas_dir(
            root,
            path,
            snapshot,
            read_file,
            case_insensitive,
            source_scope,
            ancestors,
        );
        ancestors.pop();
        return;
    }
    if !metadata.is_file() {
        record_canvas_capture_failure(
            snapshot,
            path,
            "inspect symlink target",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "symlink target is not a file or directory",
            ),
            blocks_canvas,
        );
        return;
    }
    let bytes = match read_file(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record_canvas_capture_failure(snapshot, path, "read file", error, blocks_canvas);
            return;
        }
    };
    let Ok(relative) = path.strip_prefix(root) else {
        record_canvas_capture_failure(
            snapshot,
            path,
            "make path corpus-relative",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "symlink path is outside project",
            ),
            blocks_canvas,
        );
        return;
    };
    snapshot.fingerprint.insert(
        path.to_path_buf(),
        format!(
            "!symlink-file:{target_identity}:{}",
            uhura_base::sha256_hex(&bytes)
        ),
    );
    snapshot
        .files
        .insert(relative.to_path_buf(), Arc::from(bytes));
}

fn capture_declared_dependencies(
    root: &Path,
    snapshot: &mut CanvasSourceSnapshot,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    case_insensitive: bool,
) {
    observe_declared_dependency(
        root,
        Path::new("uhura.toml"),
        snapshot,
        read_file,
        case_insensitive,
    );
    let manifest_text = snapshot
        .files
        .resolve(Path::new("uhura.toml"))
        .ok()
        .flatten()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(str::to_owned);
    let Some(manifest) = manifest_text
        .as_deref()
        .and_then(|text| load_manifest(text).ok())
    else {
        return;
    };

    let mut declared = vec![
        PathBuf::from("styles/theme.css"),
        PathBuf::from("uhura.lock"),
        PathBuf::from(&manifest.catalog_path),
    ];
    declared.extend(manifest.ports.values().map(PathBuf::from));
    declared.extend(manifest.fixtures.values().map(PathBuf::from));
    if let Some(asset_manifest) = manifest.assets_manifest.as_deref() {
        declared.push(PathBuf::from(asset_manifest));
    }
    for path in &declared {
        observe_declared_dependency(root, path, snapshot, read_file, case_insensitive);
    }

    let Some(asset_manifest) = manifest.assets_manifest.as_deref() else {
        return;
    };
    let Ok(Some(bytes)) = snapshot.files.resolve(Path::new(asset_manifest)) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return;
    };
    let Some(assets) = table.get("assets").and_then(toml::Value::as_table) else {
        return;
    };
    let asset_dir = Path::new(asset_manifest).parent().unwrap_or(Path::new(""));
    let asset_paths = assets
        .values()
        .filter_map(|entry| entry.get("file").and_then(toml::Value::as_str))
        .map(|file| asset_dir.join(file))
        .collect::<Vec<_>>();
    for path in asset_paths {
        observe_declared_dependency(root, &path, snapshot, read_file, case_insensitive);
    }
}

fn observe_declared_dependency(
    root: &Path,
    path: &Path,
    snapshot: &mut CanvasSourceSnapshot,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    case_insensitive: bool,
) {
    let Ok(normalized) = normalize_corpus_path(path) else {
        return;
    };
    mark_dependency_failures(root, &normalized, snapshot, case_insensitive);
    if path_is_excluded_from_broad_capture(&normalized, case_insensitive) {
        capture_declared_file(root, &normalized, snapshot, read_file, case_insensitive);
    }
}

fn mark_dependency_failures(
    root: &Path,
    dependency: &Path,
    snapshot: &mut CanvasSourceSnapshot,
    case_insensitive: bool,
) {
    for failure in &mut snapshot.failures {
        let Ok(failure_path) = failure.path.strip_prefix(root) else {
            continue;
        };
        if path_starts_with_components(dependency, failure_path, case_insensitive) {
            failure.blocks_canvas = true;
        }
    }
}

fn path_is_excluded_from_broad_capture(path: &Path, case_insensitive: bool) -> bool {
    path.components().enumerate().any(|(index, component)| {
        let name = component.as_os_str().to_string_lossy();
        canvas_dir_is_always_ignored(&name, case_insensitive)
            || (index == 0 && canvas_root_dir_is_generated(&name, case_insensitive))
    })
}

fn capture_declared_file(
    root: &Path,
    relative: &Path,
    snapshot: &mut CanvasSourceSnapshot,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    case_insensitive: bool,
) {
    if snapshot
        .files
        .resolve(relative)
        .is_ok_and(|bytes| bytes.is_some())
    {
        return;
    }
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let final_component = index + 1 == components.len();
        let entry = match find_canvas_child(&current, component.as_os_str(), case_insensitive) {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                snapshot.fingerprint.insert(
                    root.join(relative),
                    "!missing:declared-canvas-dependency".to_string(),
                );
                return;
            }
            Err(error) => {
                record_canvas_capture_failure(
                    snapshot,
                    &current,
                    "read declared dependency directory",
                    error,
                    true,
                );
                return;
            }
        };
        current = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                record_canvas_capture_failure(
                    snapshot,
                    &current,
                    "inspect declared dependency",
                    error,
                    true,
                );
                return;
            }
        };
        let mut target_kind = file_type;
        let mut link_identity = None;
        if file_type.is_symlink() {
            let link_target = match std::fs::read_link(&current) {
                Ok(target) => target,
                Err(error) => {
                    record_canvas_capture_failure(
                        snapshot,
                        &current,
                        "read declared dependency symlink",
                        error,
                        true,
                    );
                    return;
                }
            };
            let canonical = match std::fs::canonicalize(&current) {
                Ok(canonical) => canonical,
                Err(error) => {
                    record_canvas_capture_failure(
                        snapshot,
                        &current,
                        "resolve declared dependency symlink",
                        error,
                        true,
                    );
                    return;
                }
            };
            if !canonical.starts_with(root) {
                record_canvas_capture_failure(
                    snapshot,
                    &current,
                    "keep declared dependency symlink inside project",
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "symlink target escapes project",
                    ),
                    true,
                );
                return;
            }
            let metadata = match std::fs::metadata(&current) {
                Ok(metadata) => metadata,
                Err(error) => {
                    record_canvas_capture_failure(
                        snapshot,
                        &current,
                        "inspect declared dependency symlink target",
                        error,
                        true,
                    );
                    return;
                }
            };
            target_kind = metadata.file_type();
            link_identity = Some(format!("{:?}", link_target.as_os_str()));
            if !final_component {
                snapshot.fingerprint.insert(
                    current.clone(),
                    format!(
                        "!symlink-directory:{}",
                        link_identity.as_deref().unwrap_or_default()
                    ),
                );
            }
        }
        if !final_component {
            if !target_kind.is_dir() {
                record_canvas_capture_failure(
                    snapshot,
                    &current,
                    "traverse declared dependency",
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "path component is not a directory",
                    ),
                    true,
                );
                return;
            }
            continue;
        }
        if !target_kind.is_file() {
            record_canvas_capture_failure(
                snapshot,
                &current,
                "read declared dependency",
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "declared dependency is not a file",
                ),
                true,
            );
            return;
        }
        let bytes = match read_file(&current) {
            Ok(bytes) => bytes,
            Err(error) => {
                record_canvas_capture_failure(
                    snapshot,
                    &current,
                    "read declared dependency",
                    error,
                    true,
                );
                return;
            }
        };
        let fingerprint = match link_identity {
            Some(target) => format!("!symlink-file:{target}:{}", uhura_base::sha256_hex(&bytes)),
            None => uhura_base::sha256_hex(&bytes),
        };
        snapshot.fingerprint.insert(current.clone(), fingerprint);
        let Ok(relative) = current.strip_prefix(root) else {
            return;
        };
        snapshot
            .files
            .insert(relative.to_path_buf(), Arc::from(bytes));
    }
}

fn find_canvas_child(
    dir: &Path,
    wanted: &std::ffi::OsStr,
    case_insensitive: bool,
) -> io::Result<Option<std::fs::DirEntry>> {
    let mut alias = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name() == wanted {
            return Ok(Some(entry));
        }
        let matches_case_alias = case_insensitive
            && entry
                .file_name()
                .to_str()
                .zip(wanted.to_str())
                .is_some_and(|(actual, wanted)| actual.eq_ignore_ascii_case(wanted));
        if matches_case_alias {
            if alias.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ambiguous case-insensitive path",
                ));
            }
            alias = Some(entry);
        }
    }
    Ok(alias)
}

fn path_components_equal(left: &Path, right: &Path, case_insensitive: bool) -> bool {
    let left = left.components().collect::<Vec<_>>();
    let right = right.components().collect::<Vec<_>>();
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            canvas_name_matches(
                left.as_os_str().to_string_lossy().as_ref(),
                right.as_os_str().to_string_lossy().as_ref(),
                case_insensitive,
            )
        })
}

fn path_starts_with_components(path: &Path, prefix: &Path, case_insensitive: bool) -> bool {
    let path = path.components().collect::<Vec<_>>();
    let prefix = prefix.components().collect::<Vec<_>>();
    prefix.len() <= path.len()
        && path.iter().zip(prefix).all(|(path, prefix)| {
            canvas_name_matches(
                path.as_os_str().to_string_lossy().as_ref(),
                prefix.as_os_str().to_string_lossy().as_ref(),
                case_insensitive,
            )
        })
}

fn record_canvas_capture_failure(
    snapshot: &mut CanvasSourceSnapshot,
    path: &Path,
    operation: &'static str,
    error: io::Error,
    blocks_canvas: bool,
) {
    let identity = canvas_io_error_fingerprint(operation, &error);
    snapshot.fingerprint.insert(path.to_path_buf(), identity);
    snapshot.failures.push(CanvasCaptureFailure {
        path: path.to_path_buf(),
        operation,
        message: error.to_string(),
        blocks_canvas,
    });
}

pub(crate) fn canvas_io_error_fingerprint(operation: &str, error: &io::Error) -> String {
    format!(
        "!error:{operation}:{:?}:{:?}",
        error.kind(),
        error.raw_os_error()
    )
}

fn detect_case_insensitive_filesystem(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in &entries {
        let file_name = entry.file_name();
        let Some(alias_name) = toggle_ascii_case(&file_name) else {
            continue;
        };
        let actual = entry.path();
        let alias = root.join(alias_name);
        // A case-sensitive directory may contain a distinct alias, including
        // a hard link to the same inode. Seeing that exact directory entry is
        // conclusive; same-file identity alone would misclassify it.
        let alias_is_distinct_entry = entries
            .iter()
            .any(|entry| entry.file_name() == alias.file_name().unwrap_or_default());
        if alias_is_distinct_entry {
            return false;
        }
        let actual = match std::fs::metadata(&actual) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let alias = match std::fs::metadata(&alias) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return false,
            Err(_) => continue,
        };
        return same_file_identity(&actual, &alias);
    }
    false
}

#[cfg(unix)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    match (
        left.volume_serial_number(),
        left.file_index(),
        right.volume_serial_number(),
        right.file_index(),
    ) {
        (Some(left_volume), Some(left_index), Some(right_volume), Some(right_index)) => {
            left_volume == right_volume && left_index == right_index
        }
        _ => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
        && left.created().ok() == right.created().ok()
}

fn toggle_ascii_case(name: &std::ffi::OsStr) -> Option<OsString> {
    let name = name.to_str()?;
    let (index, character) = name
        .char_indices()
        .find(|(_, character)| character.is_ascii_alphabetic())?;
    let mut alias = name.to_string();
    let replacement = if character.is_ascii_lowercase() {
        character.to_ascii_uppercase()
    } else {
        character.to_ascii_lowercase()
    };
    alias.replace_range(
        index..index + character.len_utf8(),
        &replacement.to_string(),
    );
    Some(alias.into())
}

/// Build from one immutable observer snapshot. The Editor uses this path so
/// one candidate cannot mix bytes from separate filesystem revisions.
pub(crate) fn build_snapshot(
    files: &CanvasSourceFiles,
) -> Result<CanvasArtifact, CanvasBuildFailure> {
    let input = assemble_snapshot_input(files)?;
    build_input(input, |manifest| load_snapshot_assets(files, manifest))
}

fn build_input(
    input: CheckInput,
    load_canvas_assets: impl FnOnce(Option<&str>) -> Result<BTreeMap<String, Asset>, String>,
) -> Result<CanvasArtifact, CanvasBuildFailure> {
    let output = check(&input);
    if output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(CanvasBuildFailure::check(
            &output.diagnostics,
            &output.source_map,
        ));
    }
    let Some(lowered) = &output.lowered else {
        return Err(CanvasBuildFailure::build(
            1,
            "canvas/check",
            "the check produced no program".to_string(),
            "no checked program".to_string(),
        ));
    };
    let program = &lowered.program;
    let warnings = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .cloned()
        .collect::<Vec<_>>();
    let warnings = (!warnings.is_empty()).then(|| to_envelope(&warnings, &output.source_map));

    // ── previews → frames ──────────────────────────────────────────────
    let mut frames = Vec::new();
    let mut derived = 0usize;
    for preview in &output.previews {
        let (kind, subject) = match &preview.subject {
            SubjectKind::Page { route } => (FrameKind::Page, route.to_string()),
            SubjectKind::Surface { name, .. } => (FrameKind::Surface, name.to_string()),
            SubjectKind::Component { name } => (FrameKind::Component, name.to_string()),
        };
        if preview.derived {
            derived += 1;
        }
        let content = match &preview.payload {
            PreviewPayload::Page { u, x, .. } => match eval_view(program, u, x) {
                Ok(snapshot) => FrameContent::Snapshot(snapshot),
                Err(e) => {
                    let message = format!("{subject}/{}: {e}", preview.example);
                    return Err(CanvasBuildFailure::build(
                        1,
                        "canvas/evaluate",
                        message.clone(),
                        message,
                    ));
                }
            },
            PreviewPayload::Fragment {
                surface,
                name,
                props,
                state,
                x,
            } => {
                let def = if *surface {
                    program.surfaces.get(name)
                } else {
                    program.components.get(name)
                };
                let Some(def) = def else {
                    let message = format!("no definition `{name}`");
                    return Err(CanvasBuildFailure::build(
                        1,
                        "canvas/evaluate",
                        message.clone(),
                        message,
                    ));
                };
                match eval_fragment(program, def, props, state, x) {
                    Ok(node) => FrameContent::Fragment(node),
                    Err(e) => {
                        let message = format!("{subject}/{}: {e}", preview.example);
                        return Err(CanvasBuildFailure::build(
                            1,
                            "canvas/evaluate",
                            message.clone(),
                            message,
                        ));
                    }
                }
            }
        };
        frames.push(PreviewFrame {
            kind,
            subject,
            example: preview.example.clone(),
            is_default: preview.is_default,
            pinned: preview.pinned,
            derived: preview.derived,
            in_flight: preview.in_flight,
            from: preview.from.clone(),
            note: preview.note.clone(),
            data: preview
                .data
                .iter()
                .map(|item| PreviewField {
                    group: match item.kind {
                        PreviewDataKind::Property => PreviewFieldGroup::Properties,
                        PreviewDataKind::PageAddress => PreviewFieldGroup::PageAddress,
                        PreviewDataKind::ProvidedData => PreviewFieldGroup::ProvidedData,
                    },
                    name: item.name.to_string(),
                    key: item.key.clone(),
                    value: match &item.value {
                        PreviewDataValue::Ready(value) => PreviewFieldValue::Ready(value.clone()),
                        PreviewDataValue::Waiting => PreviewFieldValue::Waiting,
                        PreviewDataValue::Failed(reason) => {
                            PreviewFieldValue::Failed(reason.clone())
                        }
                    },
                    source: item
                        .origin
                        .as_ref()
                        .map(|origin| source_label(origin, &preview.example)),
                })
                .collect(),
            content,
        });
    }

    // ── assets: manifest + JPEG bytes → data URIs ──────────────────────
    let assets = match load_canvas_assets(input.manifest.assets_manifest.as_deref()) {
        Ok(a) => a,
        Err(e) => {
            return Err(CanvasBuildFailure::build(
                2,
                "canvas/assets",
                format!("assets: {e}"),
                format!("assets: {e}"),
            ));
        }
    };

    let html = render_canvas(
        input.manifest.app_name.as_str(),
        &frames,
        &output.stylesheet,
        &assets,
    );

    Ok(CanvasArtifact {
        html,
        preview_count: frames.len(),
        replay_derived_count: derived,
        warnings,
    })
}

fn assemble_snapshot_input(files: &CanvasSourceFiles) -> Result<CheckInput, CanvasBuildFailure> {
    let required_text = |rel: &str| -> Result<String, CanvasBuildFailure> {
        let bytes = files
            .resolve(Path::new(rel))
            .map_err(|error| CanvasBuildFailure::build(2, "canvas/input", error.clone(), error))?;
        let Some(bytes) = bytes else {
            let message = format!("{rel}: missing from the captured project snapshot");
            return Err(CanvasBuildFailure::build(
                2,
                "canvas/input",
                message.clone(),
                message,
            ));
        };
        String::from_utf8(bytes.to_vec()).map_err(|error| {
            let message = format!("{rel}: source is not UTF-8: {error}");
            CanvasBuildFailure::build(2, "canvas/input", message.clone(), message)
        })
    };
    let optional_text = |rel: &str| -> Result<Option<String>, CanvasBuildFailure> {
        let bytes = files
            .resolve(Path::new(rel))
            .map_err(|error| CanvasBuildFailure::build(2, "canvas/input", error.clone(), error))?;
        Ok(bytes
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::to_owned))
    };

    let manifest_text = required_text("uhura.toml")?;
    let manifest = load_manifest(&manifest_text).map_err(|issues| {
        let detail = issues
            .iter()
            .map(|issue| format!("{}: {}", issue.path, issue.message))
            .collect::<Vec<_>>()
            .join("; ");
        CanvasBuildFailure::build(
            1,
            "canvas/input",
            format!("uhura.toml: {detail}"),
            format!("uhura.toml: {detail}"),
        )
    })?;

    let catalog_file = (
        manifest.catalog_path.clone(),
        optional_text(&manifest.catalog_path)?,
    );
    let port_files = manifest
        .ports
        .iter()
        .map(|(name, rel)| Ok((name.clone(), (rel.clone(), optional_text(rel)?))))
        .collect::<Result<BTreeMap<_, _>, CanvasBuildFailure>>()?;
    let theme_css =
        optional_text("styles/theme.css")?.map(|css| ("styles/theme.css".to_string(), css));
    let fixture_files = manifest
        .fixtures
        .iter()
        .map(|(name, rel)| Ok((name.clone(), (rel.clone(), optional_text(rel)?))))
        .collect::<Result<BTreeMap<_, _>, CanvasBuildFailure>>()?;
    let lock_text = optional_text("uhura.lock")?;

    let mut sources = Vec::new();
    for (path, bytes) in files.iter() {
        let Some((name, logical_root)) = snapshot_source_name(path, files.case_insensitive) else {
            continue;
        };
        let text = std::str::from_utf8(bytes).map_err(|error| {
            let message = format!("{}: source is not UTF-8: {error}", path.display());
            CanvasBuildFailure::build(2, "canvas/input", message.clone(), message)
        })?;
        let kind = if name.ends_with(".examples.uhura") {
            SourceKind::Examples
        } else {
            SourceKind::Module
        };
        sources.push(SourceInput {
            rel_path: snapshot_rel_path(path, logical_root),
            text: text.to_string(),
            kind,
        });
    }
    sources.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
    if sources.is_empty() {
        let message = "no .uhura sources in the captured project snapshot".to_string();
        return Err(CanvasBuildFailure::build(
            2,
            "canvas/input",
            message.clone(),
            message,
        ));
    }

    Ok(CheckInput {
        manifest,
        manifest_rel_path: "uhura.toml".to_string(),
        manifest_text,
        catalog_file,
        port_files,
        sources,
        theme_css,
        fixture_files,
        lock_text,
    })
}

fn snapshot_source_name(path: &Path, case_insensitive: bool) -> Option<(&str, &'static str)> {
    let mut components = path.components();
    let root = components.next()?.as_os_str().to_str()?;
    let logical_root = ["app", "components", "surfaces"]
        .into_iter()
        .find(|expected| canvas_name_matches(root, expected, case_insensitive))?;
    let name = path.file_name()?.to_str()?;
    name.ends_with(".uhura").then_some((name, logical_root))
}

fn snapshot_rel_path(path: &Path, logical_root: &str) -> String {
    path.components()
        .enumerate()
        .map(|(index, component)| {
            if index == 0 {
                logical_root.to_string()
            } else {
                component.as_os_str().to_string_lossy().into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn source_label(origin: &PreviewOrigin, selected_example: &str) -> String {
    let inherited = origin
        .declared_in
        .as_deref()
        .filter(|declared| *declared != selected_example);
    match &origin.source {
        PreviewSource::Inline if origin.timeline => origin
            .declared_in
            .as_deref()
            .map(|example| format!("Calculated by “{example}” example steps"))
            .unwrap_or_else(|| "Calculated by example steps".to_string()),
        PreviewSource::Inline => inherited
            .map(|example| format!("Inherited from “{example}”"))
            .unwrap_or_else(|| "Set in this example".to_string()),
        PreviewSource::Fixture { fixture, path } => {
            let mut label = sample_data_label("From", fixture, path);
            if origin.timeline {
                if let Some(example) = origin.declared_in.as_deref() {
                    label.push_str(&format!(" · updated by “{example}” steps"));
                }
            } else if let Some(example) = inherited {
                label.push_str(&format!(" · via “{example}”"));
            }
            label
        }
        PreviewSource::AutomaticFixture { fixture, path } => {
            sample_data_label("Automatically from", fixture, path)
        }
    }
}

fn sample_data_label(prefix: &str, fixture: &str, path: &[String]) -> String {
    let path = path
        .iter()
        .map(|segment| friendly_name(segment))
        .collect::<Vec<_>>()
        .join(" · ");
    format!("{prefix} {} sample data · {path}", friendly_name(fixture))
}

fn friendly_name(name: &str) -> String {
    let words = name.replace('-', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn load_snapshot_assets(
    files: &CanvasSourceFiles,
    manifest_rel: Option<&str>,
) -> Result<BTreeMap<String, Asset>, String> {
    let mut out = BTreeMap::new();
    let Some(manifest_rel) = manifest_rel else {
        return Ok(out);
    };
    let manifest_path = Path::new(manifest_rel);
    let bytes = files.resolve(manifest_path)?.ok_or_else(|| {
        format!(
            "{}: missing from the captured snapshot",
            manifest_path.display()
        )
    })?;
    let text = std::str::from_utf8(bytes)
        .map_err(|error| format!("{}: not UTF-8: {error}", manifest_path.display()))?;
    let table: toml::Table = text.parse().map_err(|error| format!("manifest: {error}"))?;
    let Some(assets) = table.get("assets").and_then(toml::Value::as_table) else {
        return Ok(out);
    };
    let asset_dir = manifest_path.parent().unwrap_or(Path::new(""));
    for (id, entry) in assets {
        let file = entry.get("file").and_then(toml::Value::as_str);
        let alt = entry.get("alt").and_then(toml::Value::as_str);
        let (Some(file), Some(alt)) = (file, alt) else {
            return Err(format!("asset `{id}` needs `file` and `alt` (§8.3)"));
        };
        // Missing files retain the renderer's deterministic duotone fallback.
        if let Some(bytes) = files.resolve(&asset_dir.join(file))? {
            out.insert(
                id.clone(),
                Asset {
                    data_uri: format!("data:image/jpeg;base64,{}", base64(bytes)),
                    alt: alt.to_string(),
                },
            );
        }
    }
    Ok(out)
}

/// Standard base64 (padded) — 20 lines beats a dependency.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use uhura_base::{Diagnostic, SourceMap, Span};

    use super::{
        CanvasBuildFailure, CanvasSourceFiles, build_captured_snapshot, build_snapshot,
        canvas_indeterminate_path_blocks, canvas_path_blocks, canvas_scan_root,
        capture_canvas_snapshot, capture_canvas_snapshot_with, failure_envelope, snapshot_rel_path,
        snapshot_source_name,
    };

    fn corpus_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("examples/instagram-uhura")
    }

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("uhura-{label}-{}-{unique}", std::process::id()))
    }

    fn copy_tree(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).expect("copy destination");
        for entry in std::fs::read_dir(source).expect("copy source") {
            let entry = entry.expect("copy entry");
            let destination = destination.join(entry.file_name());
            if entry.file_type().expect("copy file type").is_dir() {
                copy_tree(&entry.path(), &destination);
            } else {
                std::fs::copy(entry.path(), destination).expect("copy file");
            }
        }
    }

    #[test]
    fn operational_canvas_failures_use_the_standard_diagnostics_envelope() {
        let envelope = failure_envelope("canvas/assets", "assets: manifest is invalid");

        assert_eq!(envelope["format"], "uhura-diagnostics");
        assert_eq!(envelope["version"], 0);
        assert_eq!(envelope["summary"]["errors"], 1);
        assert_eq!(envelope["diagnostics"][0]["code"], "UH9000");
        assert_eq!(envelope["diagnostics"][0]["rule"], "canvas/assets");
    }

    #[test]
    fn rejected_checks_keep_native_codes_and_source_spans() {
        let mut source_map = SourceMap::new();
        let file = source_map.add("app/broken.uhura", "page broken\n");
        let diagnostic = Diagnostic::error(
            "UH0100",
            "syntax/test",
            "broken source",
            Span::new(file, 5, 11),
        );

        let failure = CanvasBuildFailure::check(&[diagnostic], &source_map);

        assert_eq!(failure.envelope["diagnostics"][0]["code"], "UH0100");
        assert_eq!(
            failure.envelope["diagnostics"][0]["file"],
            "app/broken.uhura"
        );
        assert_eq!(failure.envelope["diagnostics"][0]["span"]["offset"], 5);
    }

    #[test]
    fn snapshot_references_normalize_inside_the_corpus_and_reject_escapes() {
        let mut files = CanvasSourceFiles::default();
        files.insert(
            PathBuf::from("catalog/base.toml"),
            Arc::from(&b"catalog"[..]),
        );
        files.insert(
            PathBuf::from("fixtures/assets/avatar.jpg"),
            Arc::from(&b"image"[..]),
        );

        assert_eq!(
            files
                .resolve(Path::new("./catalog/../catalog/base.toml"))
                .expect("safe path")
                .map(AsRef::as_ref),
            Some(&b"catalog"[..])
        );
        assert_eq!(
            files
                .resolve(Path::new("fixtures/assets/../assets/avatar.jpg"))
                .expect("contained parent path")
                .map(AsRef::as_ref),
            Some(&b"image"[..])
        );
        assert!(
            files
                .resolve(Path::new("../../outside.toml"))
                .expect_err("escaping paths are rejected")
                .contains("escapes")
        );
        assert!(
            files
                .resolve(Path::new("/outside.toml"))
                .expect_err("absolute paths are rejected")
                .contains("relative")
        );
    }

    #[test]
    fn normalized_manifest_and_asset_paths_build_the_same_instagram_canvas() {
        let root = corpus_root();
        let baseline = capture_canvas_snapshot(&root);
        let expected = build_captured_snapshot(&baseline).expect("baseline Canvas");
        let mut normalized = baseline.clone();

        let manifest = std::str::from_utf8(
            normalized
                .files
                .entries
                .get(Path::new("uhura.toml"))
                .expect("manifest"),
        )
        .expect("UTF-8 manifest")
        .replace(
            "path = \"catalog/base.toml\"",
            "path = \"./catalog/../catalog/base.toml\"",
        );
        normalized.files.insert(
            PathBuf::from("uhura.toml"),
            Arc::from(manifest.clone().into_bytes()),
        );

        let asset_manifest_path = PathBuf::from("fixtures/assets/manifest.toml");
        let asset_manifest = std::str::from_utf8(
            normalized
                .files
                .entries
                .get(&asset_manifest_path)
                .expect("asset manifest"),
        )
        .expect("UTF-8 asset manifest")
        .replacen(
            "file = \"avatar-mira.jpg\"",
            "file = \"../assets/avatar-mira.jpg\"",
            1,
        );
        normalized.files.insert(
            asset_manifest_path,
            Arc::from(asset_manifest.clone().into_bytes()),
        );

        let actual = build_snapshot(&normalized.files).expect("normalized Canvas");
        assert_eq!(actual.html, expected.html);

        let escaping_manifest = manifest.replace(
            "path = \"./catalog/../catalog/base.toml\"",
            "path = \"../outside.toml\"",
        );
        normalized.files.insert(
            PathBuf::from("uhura.toml"),
            Arc::from(escaping_manifest.into_bytes()),
        );
        let failure = match build_snapshot(&normalized.files) {
            Err(failure) => failure,
            Ok(_) => panic!("escaping path unexpectedly built"),
        };
        assert!(
            failure.envelope["diagnostics"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("escapes"))
        );

        normalized.files.insert(
            PathBuf::from("uhura.toml"),
            Arc::from(manifest.into_bytes()),
        );
        let escaping_asset_manifest = asset_manifest.replace(
            "file = \"../assets/avatar-mira.jpg\"",
            "file = \"../../../outside.jpg\"",
        );
        normalized.files.insert(
            PathBuf::from("fixtures/assets/manifest.toml"),
            Arc::from(escaping_asset_manifest.into_bytes()),
        );
        let failure = match build_snapshot(&normalized.files) {
            Err(failure) => failure,
            Ok(_) => panic!("escaping asset path unexpectedly built"),
        };
        assert_eq!(failure.envelope["diagnostics"][0]["rule"], "canvas/assets");
        assert!(
            failure.envelope["diagnostics"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("escapes"))
        );
    }

    #[test]
    fn captured_lookup_matches_the_filesystems_case_behavior() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("uhura-canvas-case-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&root).expect("test directory");
        std::fs::write(root.join("MixedCase.toml"), "value = 1").expect("mixed-case file");
        std::fs::create_dir_all(root.join("App")).expect("case-variant source directory");
        std::fs::write(root.join("App/example.uhura"), "source").expect("case-variant source");
        std::fs::create_dir_all(root.join("Build")).expect("case-variant generated directory");
        std::fs::write(root.join("Build/generated.txt"), "generated").expect("case-variant output");
        #[cfg(unix)]
        std::os::unix::fs::symlink("missing-target", root.join("A-indeterminate"))
            .expect("indeterminate first case probe");

        let snapshot = capture_canvas_snapshot(&root);
        let disk_alias_exists = std::fs::read(root.join("mixedcase.toml")).is_ok();
        let snapshot_alias_exists = snapshot
            .files
            .resolve(Path::new("mixedcase.toml"))
            .expect("safe alias")
            .is_some();
        assert_eq!(snapshot_alias_exists, disk_alias_exists);

        let disk_logical_app_exists = root.join("app").is_dir();
        let captured_source = Path::new("App/example.uhura");
        let snapshot_source =
            snapshot_source_name(captured_source, snapshot.files.case_insensitive);
        assert_eq!(snapshot_source.is_some(), disk_logical_app_exists);
        if let Some((_, logical_root)) = snapshot_source {
            assert_eq!(
                snapshot_rel_path(captured_source, logical_root),
                "app/example.uhura"
            );
        }

        let disk_logical_build_exists = root.join("build").is_dir();
        assert_eq!(
            snapshot
                .files
                .entries
                .contains_key(Path::new("Build/generated.txt")),
            !disk_logical_build_exists
        );

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn file_read_failures_reject_instead_of_disappearing_from_the_snapshot() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uhura-canvas-read-error-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("components")).expect("source directory");
        std::fs::write(root.join("uhura.toml"), "manifest").expect("manifest");
        std::fs::write(
            root.join("components/unreadable.uhura"),
            "component unreadable",
        )
        .expect("source");

        let observed_root = canvas_scan_root(&root);
        let unreadable = observed_root.join("components/unreadable.uhura");
        let snapshot = capture_canvas_snapshot_with(&root, &mut |path: &Path| {
            if path == unreadable {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected read denial",
                ))
            } else {
                std::fs::read(path)
            }
        });

        assert!(snapshot.fingerprint.contains_key(&unreadable));
        assert_eq!(snapshot.failures.len(), 1);
        assert_ne!(
            snapshot.fingerprint,
            capture_canvas_snapshot(&root).fingerprint
        );
        let failure = match build_captured_snapshot(&snapshot) {
            Err(failure) => failure,
            Ok(_) => panic!("capture error unexpectedly built"),
        };
        assert_eq!(failure.envelope["diagnostics"][0]["rule"], "canvas/input");
        assert!(
            failure.envelope["diagnostics"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("injected read denial"))
        );

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn unrelated_read_failures_remain_observable_without_rejecting_canvas() {
        let root = corpus_root();
        let observed_root = canvas_scan_root(&root);
        let unrelated = observed_root.join("README.md");
        let snapshot = capture_canvas_snapshot_with(&root, &mut |path: &Path| {
            if path == unrelated {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected unrelated denial",
                ))
            } else {
                std::fs::read(path)
            }
        });

        assert!(snapshot.fingerprint.contains_key(&unrelated));
        assert!(
            snapshot
                .failures
                .iter()
                .any(|failure| failure.path == unrelated && !failure.blocks_canvas)
        );
        build_captured_snapshot(&snapshot).expect("unrelated failure must not reject Canvas");
    }

    #[cfg(unix)]
    #[test]
    fn unknown_source_entry_failures_block_until_an_ordinary_file_kind_is_known() {
        use std::os::unix::fs::symlink;

        let root = temp_root("indeterminate-source-entry");
        copy_tree(&corpus_root(), &root);
        let observed_root = canvas_scan_root(&root);
        let unknown_source = observed_root.join("app/pending-save");
        let ordinary_source_file = observed_root.join("components/notes.txt");
        let unknown_root_file = observed_root.join("pending-save");

        assert!(canvas_indeterminate_path_blocks(
            &observed_root,
            &unknown_source,
            true,
            false,
        ));
        assert!(!canvas_path_blocks(
            &observed_root,
            &ordinary_source_file,
            true,
            false,
        ));
        assert!(!canvas_indeterminate_path_blocks(
            &observed_root,
            &unknown_root_file,
            false,
            false,
        ));

        symlink("missing-target", root.join("app/pending-save")).expect("dangling source symlink");
        let unknown_source_snapshot = capture_canvas_snapshot(&root);
        assert!(
            unknown_source_snapshot
                .failures
                .iter()
                .any(|failure| failure.path == unknown_source && failure.blocks_canvas)
        );
        assert!(build_captured_snapshot(&unknown_source_snapshot).is_err());
        std::fs::remove_file(root.join("app/pending-save")).expect("remove source symlink");

        std::fs::write(root.join("components/notes.txt"), "designer notes")
            .expect("ordinary source-adjacent file");
        let ordinary_file_snapshot = capture_canvas_snapshot_with(&root, &mut |path: &Path| {
            if path == ordinary_source_file {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected ordinary-file denial",
                ))
            } else {
                std::fs::read(path)
            }
        });
        assert!(
            ordinary_file_snapshot
                .failures
                .iter()
                .any(|failure| failure.path == ordinary_source_file && !failure.blocks_canvas)
        );
        build_captured_snapshot(&ordinary_file_snapshot)
            .expect("known ordinary file failure must not reject Canvas");

        let ignored_link = root.join("components/node_modules");
        let observed_ignored_link = observed_root.join("components/node_modules");
        symlink("missing-target", &ignored_link).expect("dangling ignored symlink");
        let ignored_link_snapshot = capture_canvas_snapshot(&root);
        assert!(
            ignored_link_snapshot
                .failures
                .iter()
                .any(|failure| failure.path == observed_ignored_link && !failure.blocks_canvas)
        );
        build_captured_snapshot(&ignored_link_snapshot)
            .expect("indeterminate ignored-name entry must not reject Canvas");

        symlink("missing-target", root.join("pending-save")).expect("dangling unrelated symlink");
        let unknown_root_snapshot = capture_canvas_snapshot(&root);
        assert!(
            unknown_root_snapshot
                .failures
                .iter()
                .any(|failure| failure.path == unknown_root_file && !failure.blocks_canvas)
        );
        build_captured_snapshot(&unknown_root_snapshot)
            .expect("unrelated indeterminate entry must not reject Canvas");

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn declared_catalog_under_generated_root_is_captured_and_recovers() {
        let root = temp_root("declared-generated");
        copy_tree(&corpus_root(), &root);
        std::fs::create_dir_all(root.join("build")).expect("generated directory");
        std::fs::copy(root.join("catalog/base.toml"), root.join("build/base.toml"))
            .expect("declared generated catalog");
        std::fs::write(root.join("build/irrelevant.txt"), "tool output")
            .expect("irrelevant output");
        let manifest = std::fs::read_to_string(root.join("uhura.toml"))
            .expect("manifest")
            .replace("path = \"catalog/base.toml\"", "path = \"build/base.toml\"");
        std::fs::write(root.join("uhura.toml"), manifest).expect("generated-root manifest");

        let snapshot = capture_canvas_snapshot(&root);
        let observed_root = canvas_scan_root(&root);
        assert!(
            snapshot
                .files
                .resolve(Path::new("build/base.toml"))
                .expect("safe catalog path")
                .is_some()
        );
        assert!(
            snapshot
                .fingerprint
                .contains_key(&observed_root.join("build/base.toml"))
        );
        assert!(
            !snapshot
                .fingerprint
                .contains_key(&observed_root.join("build/irrelevant.txt"))
        );
        build_captured_snapshot(&snapshot).expect("declared generated dependency builds");

        std::fs::remove_file(root.join("build/base.toml")).expect("remove catalog");
        let missing = capture_canvas_snapshot(&root);
        assert_ne!(snapshot.fingerprint, missing.fingerprint);
        assert!(
            missing
                .fingerprint
                .contains_key(&observed_root.join("build/base.toml"))
        );
        assert!(build_captured_snapshot(&missing).is_err());

        std::fs::copy(root.join("catalog/base.toml"), root.join("build/base.toml"))
            .expect("restore catalog");
        let recovered = capture_canvas_snapshot(&root);
        assert_ne!(missing.fingerprint, recovered.fingerprint);
        build_captured_snapshot(&recovered).expect("restored generated dependency recovers");

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn safe_symlinked_dependencies_build_while_cycles_and_escapes_reject_and_recover() {
        use std::os::unix::fs::symlink;

        let root = temp_root("canvas-symlinks");
        copy_tree(&corpus_root(), &root);
        let expected = build_captured_snapshot(&capture_canvas_snapshot(&root))
            .expect("baseline")
            .html;
        let original_manifest = std::fs::read_to_string(root.join("uhura.toml")).expect("manifest");

        symlink("catalog/base.toml", root.join("catalog-link.toml")).expect("safe file symlink");
        let linked_file_manifest = original_manifest.replace(
            "path = \"catalog/base.toml\"",
            "path = \"catalog-link.toml\"",
        );
        std::fs::write(root.join("uhura.toml"), &linked_file_manifest).expect("file-link manifest");
        let linked_file = capture_canvas_snapshot(&root);
        assert!(
            linked_file
                .fingerprint
                .get(&canvas_scan_root(&root).join("catalog-link.toml"))
                .is_some_and(|identity| identity.starts_with("!symlink-file:"))
        );
        assert_eq!(
            build_captured_snapshot(&linked_file)
                .expect("safe file link builds")
                .html,
            expected
        );

        std::fs::remove_file(root.join("catalog-link.toml")).expect("remove file link");
        symlink("catalog", root.join("catalog-link")).expect("safe directory symlink");
        let linked_dir_manifest = original_manifest.replace(
            "path = \"catalog/base.toml\"",
            "path = \"catalog-link/base.toml\"",
        );
        std::fs::write(root.join("uhura.toml"), &linked_dir_manifest)
            .expect("directory-link manifest");
        let linked_dir = capture_canvas_snapshot(&root);
        assert!(
            linked_dir
                .fingerprint
                .get(&canvas_scan_root(&root).join("catalog-link"))
                .is_some_and(|identity| identity.starts_with("!symlink-directory:"))
        );
        assert_eq!(
            build_captured_snapshot(&linked_dir)
                .expect("safe directory link builds")
                .html,
            expected
        );

        symlink(root.join("components"), root.join("components/cycle")).expect("source cycle");
        let cycle = capture_canvas_snapshot(&root);
        assert!(build_captured_snapshot(&cycle).is_err());
        std::fs::remove_file(root.join("components/cycle")).expect("remove cycle");

        let outside = temp_root("outside-catalog");
        std::fs::write(
            &outside,
            std::fs::read(root.join("catalog/base.toml")).expect("catalog"),
        )
        .expect("outside catalog");
        std::fs::remove_file(root.join("catalog-link")).expect("remove directory link");
        symlink(&outside, root.join("catalog-link.toml")).expect("escaping file link");
        std::fs::write(root.join("uhura.toml"), &linked_file_manifest)
            .expect("escaping-link manifest");
        let escaped = capture_canvas_snapshot(&root);
        assert!(build_captured_snapshot(&escaped).is_err());

        std::fs::remove_file(root.join("catalog-link.toml")).expect("remove escape");
        symlink("catalog/base.toml", root.join("catalog-link.toml")).expect("restore safe link");
        let recovered = capture_canvas_snapshot(&root);
        assert_ne!(escaped.fingerprint, recovered.fingerprint);
        assert_eq!(
            build_captured_snapshot(&recovered)
                .expect("safe retarget recovers")
                .html,
            expected
        );

        std::fs::remove_file(outside).expect("outside cleanup");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
