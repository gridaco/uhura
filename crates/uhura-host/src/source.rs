//! Coherent Uhura source capture and Editor read-model construction. Filesystem
//! bytes are captured once, then checking, example replay, and evaluation use
//! that immutable revision. Browser presentation lives entirely in `web/`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Read};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use uhura_base::{Severity, to_envelope};
use uhura_check::icon_fonts::{MAX_ICON_FONT_BYTES, MAX_ICON_GLYPH_MAP_BYTES};
use uhura_check::manifest::load_manifest;
use uhura_check::{CheckInput, IconFontInput, SourceInput, check};
use uhura_editor_model::{Asset, EditorRender, build_render};
use uhura_syntax::SourceKind;

use crate::IconFontResources;

/// Canonical corpus-relative file bytes captured by the project observer.
///
/// References are normalized inside this immutable keyspace. Case aliases are
/// honored only when the capture established that the backing filesystem is
/// case-insensitive, preserving disk lookup semantics without making Linux
/// projects unexpectedly case-insensitive.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectSourceFiles {
    entries: BTreeMap<PathBuf, Arc<[u8]>>,
    case_insensitive: bool,
}

impl ProjectSourceFiles {
    fn insert(&mut self, path: PathBuf, bytes: Arc<[u8]>) {
        self.entries.insert(path, bytes);
    }

    fn iter(&self) -> impl Iterator<Item = (&PathBuf, &Arc<[u8]>)> {
        self.entries.iter()
    }

    pub(crate) fn resolve(&self, path: &Path) -> Result<Option<&Arc<[u8]>>, String> {
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

    pub(crate) fn subtree(&self, prefix: &Path) -> BTreeMap<String, Arc<[u8]>> {
        self.entries
            .iter()
            .filter_map(|(path, bytes)| {
                let relative = path.strip_prefix(prefix).ok()?;
                if relative.as_os_str().is_empty() {
                    return None;
                }
                let parts = relative
                    .components()
                    .map(|component| component.as_os_str().to_str())
                    .collect::<Option<Vec<_>>>()?;
                Some((parts.join("/"), Arc::clone(bytes)))
            })
            .collect()
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
pub struct ProjectSourceFingerprint {
    entries: BTreeMap<PathBuf, String>,
    case_insensitive: bool,
}

impl Deref for ProjectSourceFingerprint {
    type Target = BTreeMap<PathBuf, String>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for ProjectSourceFingerprint {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

impl ProjectSourceFingerprint {
    /// Deterministic content identity for this complete observation.
    ///
    /// The digest includes filesystem case behavior plus length-prefixed raw
    /// path identities and values for every entry. It is suitable for host
    /// generation comparisons without relying on `Debug` formatting.
    pub fn stable_id(&self) -> String {
        let mut bytes = b"uhura-project-source-fingerprint/1\0".to_vec();
        bytes.push(u8::from(self.case_insensitive));
        bytes.extend_from_slice(&(self.entries.len() as u64).to_be_bytes());
        for (path, value) in &self.entries {
            let path = fingerprint_path_bytes(path);
            append_fingerprint_field(&mut bytes, &path);
            append_fingerprint_field(&mut bytes, value.as_bytes());
        }
        uhura_base::sha256_hex(&bytes)
    }
}

fn append_fingerprint_field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
    bytes.extend_from_slice(field);
}

#[cfg(unix)]
fn fingerprint_path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    let mut bytes = b"unix\0".to_vec();
    bytes.extend_from_slice(path.as_os_str().as_bytes());
    bytes
}

#[cfg(windows)]
fn fingerprint_path_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    let mut bytes = b"windows-utf16le\0".to_vec();
    for unit in path.as_os_str().encode_wide() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[cfg(not(any(unix, windows)))]
fn fingerprint_path_bytes(path: &Path) -> Vec<u8> {
    let mut bytes = b"unicode-lossy\0".to_vec();
    bytes.extend_from_slice(path.to_string_lossy().as_bytes());
    bytes
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ProjectCaptureFailure {
    path: PathBuf,
    operation: &'static str,
    message: String,
    blocks_build: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectSourceSnapshot {
    pub(crate) files: ProjectSourceFiles,
    pub(crate) fingerprint: ProjectSourceFingerprint,
    failures: Vec<ProjectCaptureFailure>,
}

impl ProjectSourceSnapshot {
    /// Content identity of every captured project input.
    pub fn fingerprint(&self) -> &ProjectSourceFingerprint {
        &self.fingerprint
    }
}

/// One completely checked Editor publication held in memory. The render stays
/// browser-neutral; validated renderer resources ride beside it and are never
/// serialized into `EditorState`.
pub(crate) struct EditorModelArtifact {
    pub(crate) render: EditorRender,
    pub(crate) icon_fonts: Option<IconFontResources>,
    pub(crate) preview_count: usize,
    pub(crate) replay_derived_count: usize,
    /// Diagnostics belonging to this otherwise renderable source revision.
    pub(crate) diagnostics: serde_json::Value,
}

/// A rejected in-memory Editor-model build. Hosts publish its diagnostics
/// envelope together with the candidate source revision.
#[derive(Debug)]
pub(crate) struct EditorModelBuildFailure {
    pub(crate) envelope: serde_json::Value,
}

impl EditorModelBuildFailure {
    fn check(diagnostics: &[uhura_base::Diagnostic], source_map: &uhura_base::SourceMap) -> Self {
        Self {
            envelope: to_envelope(diagnostics, source_map),
        }
    }

    fn capture(failures: &[ProjectCaptureFailure]) -> Self {
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
        Self::build(2, "editor/input", message.clone(), message)
    }

    fn build(_exit_code: u8, rule: &str, message: String, _terminal: String) -> Self {
        Self {
            envelope: failure_envelope(rule, &message),
        }
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

/// Test convenience for building one captured revision at revision 1.
#[cfg(test)]
pub(crate) fn build_captured_snapshot(
    snapshot: &ProjectSourceSnapshot,
) -> Result<EditorModelArtifact, EditorModelBuildFailure> {
    build_captured_snapshot_at(snapshot, 1)
}

pub(crate) fn build_captured_snapshot_at(
    snapshot: &ProjectSourceSnapshot,
    revision: u64,
) -> Result<EditorModelArtifact, EditorModelBuildFailure> {
    let blocking = snapshot
        .failures
        .iter()
        .filter(|failure| failure.blocks_build)
        .cloned()
        .collect::<Vec<_>>();
    if !blocking.is_empty() {
        return Err(EditorModelBuildFailure::capture(&blocking));
    }
    build_snapshot_at(&snapshot.files, revision)
}

/// Capture every observable project input once. The bytes stored here are both
/// fingerprinted and consumed by the builder, so an attempt cannot mix file
/// revisions. Safe in-project symlinks retain logical identity; broad output
/// exclusions are overridden only for exact declared dependencies.
pub fn capture_project_snapshot(root: &Path) -> ProjectSourceSnapshot {
    capture_project_snapshot_with(root, &mut |path: &Path, max_bytes| match max_bytes {
        Some(max_bytes) => read_bounded_file(path, max_bytes),
        None => std::fs::read(path),
    })
}

fn capture_project_snapshot_with(
    root: &Path,
    read_file: &mut impl FnMut(&Path, Option<usize>) -> io::Result<Vec<u8>>,
) -> ProjectSourceSnapshot {
    let root = project_scan_root(root);
    let case_insensitive = detect_case_insensitive_filesystem(&root);
    let mut snapshot = ProjectSourceSnapshot::default();
    snapshot.files.case_insensitive = case_insensitive;
    snapshot.fingerprint.case_insensitive = case_insensitive;
    {
        let mut read_manifest = |path: &Path| read_file(path, None);
        capture_declared_file(
            &root,
            Path::new("uhura.toml"),
            &mut snapshot,
            &mut read_manifest,
            case_insensitive,
        );
    }
    let icon_limits = declared_icon_resource_limits(&snapshot.files);
    let mut read_project_file = |path: &Path| {
        let max_bytes = declared_file_limit(&root, path, &icon_limits, case_insensitive);
        read_file(path, max_bytes)
    };
    let mut ancestors = vec![root.clone()];
    capture_project_dir(
        &root,
        &root,
        &mut snapshot,
        &mut read_project_file,
        case_insensitive,
        false,
        &mut ancestors,
    );
    capture_declared_dependencies(
        &root,
        &mut snapshot,
        &mut read_project_file,
        case_insensitive,
    );
    snapshot.failures.sort();
    snapshot
}

fn read_bounded_file(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > max_bytes as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file exceeds {max_bytes}-byte limit"),
        ));
    }
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file exceeds {max_bytes}-byte limit"),
        ));
    }
    Ok(bytes)
}

fn declared_icon_resource_limits(files: &ProjectSourceFiles) -> BTreeMap<PathBuf, usize> {
    let Ok(Some(bytes)) = files.resolve(Path::new("uhura.toml")) else {
        return BTreeMap::new();
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return BTreeMap::new();
    };
    let Ok(manifest) = load_manifest(text) else {
        return BTreeMap::new();
    };

    let mut limits: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for family in manifest.icons.families.values() {
        if let Ok(path) = normalize_corpus_path(Path::new(&family.font)) {
            limits
                .entry(path)
                .and_modify(|limit| *limit = (*limit).min(MAX_ICON_FONT_BYTES))
                .or_insert(MAX_ICON_FONT_BYTES);
        }
        if let Ok(path) = normalize_corpus_path(Path::new(&family.glyphs)) {
            limits
                .entry(path)
                .and_modify(|limit| *limit = (*limit).min(MAX_ICON_GLYPH_MAP_BYTES))
                .or_insert(MAX_ICON_GLYPH_MAP_BYTES);
        }
    }
    limits
}

fn declared_file_limit(
    root: &Path,
    path: &Path,
    limits: &BTreeMap<PathBuf, usize>,
    case_insensitive: bool,
) -> Option<usize> {
    let relative = path.strip_prefix(root).ok()?;
    if let Some(limit) = limits.get(relative) {
        return Some(*limit);
    }
    if !case_insensitive {
        return None;
    }
    limits.iter().find_map(|(candidate, limit)| {
        path_components_equal(relative, candidate, true).then_some(*limit)
    })
}

pub(crate) fn project_scan_root(root: &Path) -> PathBuf {
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

pub(crate) fn project_root_dir_is_generated(name: &str, case_insensitive: bool) -> bool {
    ["build", "renders", "target"]
        .into_iter()
        .any(|expected| project_name_matches(name, expected, case_insensitive))
        || project_dir_is_always_ignored(name, case_insensitive)
}

pub(crate) fn project_dir_is_always_ignored(name: &str, case_insensitive: bool) -> bool {
    ["node_modules", ".git"]
        .into_iter()
        .any(|expected| project_name_matches(name, expected, case_insensitive))
}

fn project_name_matches(actual: &str, expected: &str, case_insensitive: bool) -> bool {
    actual == expected || (case_insensitive && actual.eq_ignore_ascii_case(expected))
}

fn capture_project_dir(
    root: &Path,
    dir: &Path,
    snapshot: &mut ProjectSourceSnapshot,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    case_insensitive: bool,
    source_scope: bool,
    ancestors: &mut Vec<PathBuf>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            record_project_capture_failure(
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
                record_project_capture_failure(
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
        if snapshot.fingerprint.contains_key(&path) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let child_source_scope = source_scope
            || (dir == root
                && ["app", "components", "surfaces"]
                    .into_iter()
                    .any(|expected| project_name_matches(&name, expected, case_insensitive)));
        let indeterminate_ignored_name = project_dir_is_always_ignored(&name, case_insensitive)
            || (dir == root && project_root_dir_is_generated(&name, case_insensitive));
        let blocks_build = project_path_blocks(root, &path, child_source_scope, case_insensitive);
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                record_project_capture_failure(
                    snapshot,
                    &path,
                    "inspect file type",
                    error,
                    !indeterminate_ignored_name
                        && project_indeterminate_path_blocks(
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
            capture_project_symlink(
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
                        record_project_capture_failure(
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
                    record_project_capture_failure(
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
                    record_project_capture_failure(
                        snapshot,
                        &path,
                        "traverse directory",
                        io::Error::new(io::ErrorKind::InvalidData, "directory cycle"),
                        child_source_scope,
                    );
                    continue;
                }
                ancestors.push(canonical);
                capture_project_dir(
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
                record_project_capture_failure(snapshot, &path, "read file", error, blocks_build);
                continue;
            }
        };
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative.to_path_buf(),
            Err(error) => {
                record_project_capture_failure(
                    snapshot,
                    &path,
                    "make path corpus-relative",
                    io::Error::new(io::ErrorKind::InvalidData, error.to_string()),
                    blocks_build,
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

fn project_path_blocks(
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
/// extension/dependency policy in `project_path_blocks` applies instead.
fn project_indeterminate_path_blocks(
    root: &Path,
    path: &Path,
    source_scope: bool,
    case_insensitive: bool,
) -> bool {
    source_scope || project_path_blocks(root, path, source_scope, case_insensitive)
}

fn capture_project_symlink(
    root: &Path,
    path: &Path,
    snapshot: &mut ProjectSourceSnapshot,
    read_file: &mut impl FnMut(&Path) -> io::Result<Vec<u8>>,
    case_insensitive: bool,
    source_scope: bool,
    ancestors: &mut Vec<PathBuf>,
) {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let ignored_directory_name = project_dir_is_always_ignored(name, case_insensitive)
        || (path.parent() == Some(root) && project_root_dir_is_generated(name, case_insensitive));
    let path_blocks_build = project_path_blocks(root, path, source_scope, case_insensitive);
    let indeterminate_blocks_build = !ignored_directory_name
        && project_indeterminate_path_blocks(root, path, source_scope, case_insensitive);
    let link_target = match std::fs::read_link(path) {
        Ok(target) => target,
        Err(error) => {
            record_project_capture_failure(
                snapshot,
                path,
                "read symlink",
                error,
                indeterminate_blocks_build,
            );
            return;
        }
    };
    let target_identity = format!("{:?}", link_target.as_os_str());
    let canonical = match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(error) => {
            let error_identity = project_io_error_fingerprint("resolve symlink", &error);
            record_project_capture_failure(
                snapshot,
                path,
                "resolve symlink",
                error,
                indeterminate_blocks_build,
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
            record_project_capture_failure(
                snapshot,
                path,
                "inspect symlink target",
                error,
                indeterminate_blocks_build,
            );
            return;
        }
    };
    if metadata.is_dir() && ignored_directory_name {
        return;
    }
    let blocks_build = path_blocks_build || (source_scope && metadata.is_dir());
    if !canonical.starts_with(root) {
        record_project_capture_failure(
            snapshot,
            path,
            "keep symlink inside project",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "symlink target escapes project",
            ),
            blocks_build,
        );
        return;
    }
    if metadata.is_dir() {
        snapshot.fingerprint.insert(
            path.to_path_buf(),
            format!("!symlink-directory:{target_identity}"),
        );
        if ancestors.contains(&canonical) {
            record_project_capture_failure(
                snapshot,
                path,
                "traverse symlink",
                io::Error::new(io::ErrorKind::InvalidData, "symlink directory cycle"),
                source_scope,
            );
            return;
        }
        ancestors.push(canonical);
        capture_project_dir(
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
        record_project_capture_failure(
            snapshot,
            path,
            "inspect symlink target",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "symlink target is not a file or directory",
            ),
            blocks_build,
        );
        return;
    }
    let bytes = match read_file(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record_project_capture_failure(snapshot, path, "read file", error, blocks_build);
            return;
        }
    };
    let Ok(relative) = path.strip_prefix(root) else {
        record_project_capture_failure(
            snapshot,
            path,
            "make path corpus-relative",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "symlink path is outside project",
            ),
            blocks_build,
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
    snapshot: &mut ProjectSourceSnapshot,
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
    for family in manifest.icons.families.values() {
        declared.push(PathBuf::from(&family.font));
        declared.push(PathBuf::from(&family.glyphs));
    }
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
    snapshot: &mut ProjectSourceSnapshot,
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
    snapshot: &mut ProjectSourceSnapshot,
    case_insensitive: bool,
) {
    for failure in &mut snapshot.failures {
        let Ok(failure_path) = failure.path.strip_prefix(root) else {
            continue;
        };
        if path_starts_with_components(dependency, failure_path, case_insensitive) {
            failure.blocks_build = true;
        }
    }
}

fn path_is_excluded_from_broad_capture(path: &Path, case_insensitive: bool) -> bool {
    path.components().enumerate().any(|(index, component)| {
        let name = component.as_os_str().to_string_lossy();
        project_dir_is_always_ignored(&name, case_insensitive)
            || (index == 0 && project_root_dir_is_generated(&name, case_insensitive))
    })
}

fn capture_declared_file(
    root: &Path,
    relative: &Path,
    snapshot: &mut ProjectSourceSnapshot,
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
        let entry = match find_project_child(&current, component.as_os_str(), case_insensitive) {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                snapshot.fingerprint.insert(
                    root.join(relative),
                    "!missing:declared-project-dependency".to_string(),
                );
                return;
            }
            Err(error) => {
                record_project_capture_failure(
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
                record_project_capture_failure(
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
                    record_project_capture_failure(
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
                    record_project_capture_failure(
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
                record_project_capture_failure(
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
                    record_project_capture_failure(
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
                record_project_capture_failure(
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
            record_project_capture_failure(
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
                record_project_capture_failure(
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

fn find_project_child(
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
            project_name_matches(
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
            project_name_matches(
                path.as_os_str().to_string_lossy().as_ref(),
                prefix.as_os_str().to_string_lossy().as_ref(),
                case_insensitive,
            )
        })
}

fn record_project_capture_failure(
    snapshot: &mut ProjectSourceSnapshot,
    path: &Path,
    operation: &'static str,
    error: io::Error,
    blocks_build: bool,
) {
    let identity = project_io_error_fingerprint(operation, &error);
    snapshot.fingerprint.insert(path.to_path_buf(), identity);
    snapshot.failures.push(ProjectCaptureFailure {
        path: path.to_path_buf(),
        operation,
        message: error.to_string(),
        blocks_build,
    });
}

pub(crate) fn project_io_error_fingerprint(operation: &str, error: &io::Error) -> String {
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
        match std::fs::metadata(&actual) {
            Ok(_) => {}
            Err(_) => continue,
        }
        match std::fs::metadata(&alias) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return false,
            Err(_) => continue,
        }
        // Keep both identities live while comparing them. This is portable
        // to stable Windows, where std's by-handle metadata IDs are unstable.
        match same_file::is_same_file(&actual, &alias) {
            Ok(same_file) => return same_file,
            Err(_) => continue,
        }
    }
    false
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

/// Test convenience for building one immutable snapshot at revision 1.
#[cfg(test)]
pub(crate) fn build_snapshot(
    files: &ProjectSourceFiles,
) -> Result<EditorModelArtifact, EditorModelBuildFailure> {
    build_snapshot_at(files, 1)
}

pub(crate) fn build_snapshot_at(
    files: &ProjectSourceFiles,
    revision: u64,
) -> Result<EditorModelArtifact, EditorModelBuildFailure> {
    let input = assemble_snapshot_input(files)?;
    build_input(input, revision, |manifest| {
        load_snapshot_assets(files, manifest)
    })
}

fn build_input(
    input: CheckInput,
    revision: u64,
    load_editor_assets: impl FnOnce(Option<&str>) -> Result<BTreeMap<String, Asset>, String>,
) -> Result<EditorModelArtifact, EditorModelBuildFailure> {
    let output = check(&input);
    if output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(EditorModelBuildFailure::check(
            &output.diagnostics,
            &output.source_map,
        ));
    }
    let preview_count = output.previews.len();
    let replay_derived_count = output
        .previews
        .iter()
        .filter(|preview| preview.derived)
        .count();
    let diagnostics = uhura_editor_model::diagnostics_json(&output);

    // Assets stay revision-local: the browser never rereads project files.
    let assets = match load_editor_assets(input.manifest.assets_manifest.as_deref()) {
        Ok(a) => a,
        Err(e) => {
            return Err(EditorModelBuildFailure::build(
                2,
                "editor/assets",
                format!("assets: {e}"),
                format!("assets: {e}"),
            ));
        }
    };
    let render = build_render(revision, &output, assets).map_err(|error| {
        let message = error.to_string();
        EditorModelBuildFailure::build(1, "editor/evaluate", message.clone(), message)
    })?;

    Ok(EditorModelArtifact {
        render,
        icon_fonts: output.icon_fonts.as_ref().map(IconFontResources::from),
        preview_count,
        replay_derived_count,
        diagnostics,
    })
}

pub(crate) fn assemble_snapshot_input(
    files: &ProjectSourceFiles,
) -> Result<CheckInput, EditorModelBuildFailure> {
    let required_text = |rel: &str| -> Result<String, EditorModelBuildFailure> {
        let bytes = files.resolve(Path::new(rel)).map_err(|error| {
            EditorModelBuildFailure::build(2, "editor/input", error.clone(), error)
        })?;
        let Some(bytes) = bytes else {
            let message = format!("{rel}: missing from the captured project snapshot");
            return Err(EditorModelBuildFailure::build(
                2,
                "editor/input",
                message.clone(),
                message,
            ));
        };
        String::from_utf8(bytes.to_vec()).map_err(|error| {
            let message = format!("{rel}: source is not UTF-8: {error}");
            EditorModelBuildFailure::build(2, "editor/input", message.clone(), message)
        })
    };
    let optional_text = |rel: &str| -> Result<Option<String>, EditorModelBuildFailure> {
        let bytes = files.resolve(Path::new(rel)).map_err(|error| {
            EditorModelBuildFailure::build(2, "editor/input", error.clone(), error)
        })?;
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
        EditorModelBuildFailure::build(
            1,
            "editor/input",
            format!("uhura.toml: {detail}"),
            format!("uhura.toml: {detail}"),
        )
    })?;

    let catalog_file = (
        manifest.catalog_path.clone(),
        optional_text(&manifest.catalog_path)?,
    );
    let icon_font_files = manifest
        .icons
        .families
        .iter()
        .map(|(name, family)| {
            let font_bytes = files
                .resolve(Path::new(&family.font))
                .map_err(|error| {
                    EditorModelBuildFailure::build(2, "editor/input", error.clone(), error)
                })?
                .cloned();
            Ok((
                name.clone(),
                IconFontInput {
                    font_path: family.font.clone(),
                    font_bytes,
                    glyphs_path: family.glyphs.clone(),
                    glyphs_text: optional_text(&family.glyphs)?,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, EditorModelBuildFailure>>()?;
    let port_files = manifest
        .ports
        .iter()
        .map(|(name, rel)| Ok((name.clone(), (rel.clone(), optional_text(rel)?))))
        .collect::<Result<BTreeMap<_, _>, EditorModelBuildFailure>>()?;
    let theme_css =
        optional_text("styles/theme.css")?.map(|css| ("styles/theme.css".to_string(), css));
    let fixture_files = manifest
        .fixtures
        .iter()
        .map(|(name, rel)| Ok((name.clone(), (rel.clone(), optional_text(rel)?))))
        .collect::<Result<BTreeMap<_, _>, EditorModelBuildFailure>>()?;
    let lock_text = optional_text("uhura.lock")?;

    let mut sources = Vec::new();
    for (path, bytes) in files.iter() {
        let Some((name, logical_root)) = snapshot_source_name(path, files.case_insensitive) else {
            continue;
        };
        let text = std::str::from_utf8(bytes).map_err(|error| {
            let message = format!("{}: source is not UTF-8: {error}", path.display());
            EditorModelBuildFailure::build(2, "editor/input", message.clone(), message)
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
        return Err(EditorModelBuildFailure::build(
            2,
            "editor/input",
            message.clone(),
            message,
        ));
    }

    Ok(CheckInput {
        manifest,
        manifest_rel_path: "uhura.toml".to_string(),
        manifest_text,
        catalog_file,
        icon_font_files,
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
        .find(|expected| project_name_matches(root, expected, case_insensitive))?;
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

fn load_snapshot_assets(
    files: &ProjectSourceFiles,
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
            let media_type = asset_media_type(file);
            out.insert(
                id.clone(),
                Asset {
                    data_uri: format!("data:{media_type};base64,{}", base64(bytes)),
                    alt: alt.to_string(),
                },
            );
        }
    }
    Ok(out)
}

fn asset_media_type(file: &str) -> &'static str {
    let extension = Path::new(file)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
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

    use uhura_base::{Diagnostic, Ident, SourceMap, Span};

    use super::{
        EditorModelBuildFailure, MAX_ICON_GLYPH_MAP_BYTES, ProjectSourceFiles,
        ProjectSourceFingerprint, assemble_snapshot_input, build_captured_snapshot, build_snapshot,
        capture_project_snapshot, capture_project_snapshot_with, failure_envelope,
        load_snapshot_assets, project_indeterminate_path_blocks, project_path_blocks,
        project_scan_root, snapshot_rel_path, snapshot_source_name,
    };

    fn corpus_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("examples/instagram/client")
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
    fn fingerprint_stable_id_covers_case_behavior_paths_and_values() {
        let mut first = ProjectSourceFingerprint::default();
        first.insert(PathBuf::from("app/a.uhura"), "one".to_string());
        first.insert(PathBuf::from("app/b.uhura"), "two".to_string());

        let mut reordered = ProjectSourceFingerprint::default();
        reordered.insert(PathBuf::from("app/b.uhura"), "two".to_string());
        reordered.insert(PathBuf::from("app/a.uhura"), "one".to_string());
        assert_eq!(first.stable_id(), reordered.stable_id());

        let mut changed_value = first.clone();
        changed_value.insert(PathBuf::from("app/a.uhura"), "changed".to_string());
        assert_ne!(first.stable_id(), changed_value.stable_id());

        let mut changed_path = first.clone();
        let value = changed_path.remove(Path::new("app/a.uhura")).unwrap();
        changed_path.insert(PathBuf::from("app/c.uhura"), value);
        assert_ne!(first.stable_id(), changed_path.stable_id());

        let mut changed_case_behavior = first.clone();
        changed_case_behavior.case_insensitive = true;
        assert_ne!(first.stable_id(), changed_case_behavior.stable_id());
    }

    #[test]
    fn operational_editor_failures_use_the_standard_diagnostics_envelope() {
        let envelope = failure_envelope("editor/assets", "assets: manifest is invalid");

        assert_eq!(envelope["format"], "uhura-diagnostics");
        assert_eq!(envelope["version"], 0);
        assert_eq!(envelope["summary"]["errors"], 1);
        assert_eq!(envelope["diagnostics"][0]["code"], "UH9000");
        assert_eq!(envelope["diagnostics"][0]["rule"], "editor/assets");
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

        let failure = EditorModelBuildFailure::check(&[diagnostic], &source_map);

        assert_eq!(failure.envelope["diagnostics"][0]["code"], "UH0100");
        assert_eq!(
            failure.envelope["diagnostics"][0]["file"],
            "app/broken.uhura"
        );
        assert_eq!(failure.envelope["diagnostics"][0]["span"]["offset"], 5);
    }

    #[test]
    fn snapshot_references_normalize_inside_the_corpus_and_reject_escapes() {
        let mut files = ProjectSourceFiles::default();
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
    fn declared_local_icon_files_are_captured_and_associated_with_their_family() {
        let root = temp_root("declared-icon-fonts");
        std::fs::create_dir_all(root.join("components")).expect("source directory");
        std::fs::create_dir_all(root.join("build/icon-fonts")).expect("generated icon directory");
        std::fs::write(
            root.join("uhura.toml"),
            r#"[app]
name = "icon-capture"
entry = "home"

[catalog]
path = "catalog/base.toml"

[icons]
default = "brand"

[icons.brand]
font = "build/icon-fonts/brand.woff2"
glyphs = "build/icon-fonts/brand.json"
"#,
        )
        .expect("manifest");
        std::fs::write(root.join("components/card.uhura"), "component card").expect("source");
        std::fs::write(root.join("build/icon-fonts/brand.woff2"), b"brand-font").expect("font");
        std::fs::write(
            root.join("build/icon-fonts/brand.json"),
            r#"{"glyphs":{"home":57344}}"#,
        )
        .expect("glyph map");

        let snapshot = capture_project_snapshot(&root);
        assert!(
            snapshot
                .files
                .entries
                .contains_key(Path::new("build/icon-fonts/brand.woff2")),
            "declared font overrides the broad generated-directory exclusion",
        );
        assert!(
            snapshot
                .files
                .entries
                .contains_key(Path::new("build/icon-fonts/brand.json")),
            "declared glyph map overrides the broad generated-directory exclusion",
        );

        let input = assemble_snapshot_input(&snapshot.files).expect("assembled checker input");
        let brand = &input.icon_font_files[&Ident::new("brand").expect("valid family identifier")];
        assert_eq!(brand.font_path, "build/icon-fonts/brand.woff2");
        assert_eq!(brand.font_bytes.as_deref(), Some(&b"brand-font"[..]));
        assert_eq!(brand.glyphs_path, "build/icon-fonts/brand.json");
        assert_eq!(
            brand.glyphs_text.as_deref(),
            Some(r#"{"glyphs":{"home":57344}}"#)
        );

        let oversized_glyphs = std::fs::OpenOptions::new()
            .write(true)
            .open(root.join("build/icon-fonts/brand.json"))
            .expect("open glyph map");
        oversized_glyphs
            .set_len((MAX_ICON_GLYPH_MAP_BYTES + 1) as u64)
            .expect("make sparse oversized glyph map");
        let oversized = capture_project_snapshot(&root);
        assert!(oversized.failures.iter().any(|failure| {
            failure.blocks_build
                && failure.path.ends_with("build/icon-fonts/brand.json")
                && failure.message.contains("byte limit")
        }));

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn snapshot_assets_embed_the_media_type_from_their_file_extension() {
        let mut files = ProjectSourceFiles::default();
        files.insert(
            PathBuf::from("fixtures/assets/manifest.toml"),
            Arc::from(&b"[assets.photo]\nfile = \"photo.webp\"\nalt = \"A photo\"\n"[..]),
        );
        files.insert(
            PathBuf::from("fixtures/assets/photo.webp"),
            Arc::from(&b"webp"[..]),
        );

        let assets = load_snapshot_assets(&files, Some("fixtures/assets/manifest.toml"))
            .expect("asset manifest");

        assert_eq!(assets["photo"].data_uri, "data:image/webp;base64,d2VicA==");
    }

    #[test]
    fn normalized_manifest_and_asset_paths_build_the_same_instagram_model() {
        let root = corpus_root();
        let baseline = capture_project_snapshot(&root);
        let expected = build_captured_snapshot(&baseline).expect("baseline Editor model");
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
            "file = \"avatar-mira.webp\"",
            "file = \"../assets/avatar-mira.webp\"",
            1,
        );
        normalized.files.insert(
            asset_manifest_path,
            Arc::from(asset_manifest.clone().into_bytes()),
        );

        let actual = build_snapshot(&normalized.files).expect("normalized Editor model");
        assert_eq!(actual.render.to_json(), expected.render.to_json());

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
            "file = \"../assets/avatar-mira.webp\"",
            "file = \"../../../outside.webp\"",
        );
        normalized.files.insert(
            PathBuf::from("fixtures/assets/manifest.toml"),
            Arc::from(escaping_asset_manifest.into_bytes()),
        );
        let failure = match build_snapshot(&normalized.files) {
            Err(failure) => failure,
            Ok(_) => panic!("escaping asset path unexpectedly built"),
        };
        assert_eq!(failure.envelope["diagnostics"][0]["rule"], "editor/assets");
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
        let root = std::env::temp_dir().join(format!(
            "uhura-editor-capture-case-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("test directory");
        std::fs::write(root.join("MixedCase.toml"), "value = 1").expect("mixed-case file");
        std::fs::create_dir_all(root.join("App")).expect("case-variant source directory");
        std::fs::write(root.join("App/example.uhura"), "source").expect("case-variant source");
        std::fs::create_dir_all(root.join("Build")).expect("case-variant generated directory");
        std::fs::write(root.join("Build/generated.txt"), "generated").expect("case-variant output");
        #[cfg(unix)]
        std::os::unix::fs::symlink("missing-target", root.join("A-indeterminate"))
            .expect("indeterminate first case probe");

        let snapshot = capture_project_snapshot(&root);
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
            "uhura-editor-capture-read-error-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("components")).expect("source directory");
        std::fs::write(root.join("uhura.toml"), "manifest").expect("manifest");
        std::fs::write(
            root.join("components/unreadable.uhura"),
            "component unreadable",
        )
        .expect("source");

        let observed_root = project_scan_root(&root);
        let unreadable = observed_root.join("components/unreadable.uhura");
        let snapshot = capture_project_snapshot_with(&root, &mut |path: &Path, _max_bytes| {
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
            capture_project_snapshot(&root).fingerprint
        );
        let failure = match build_captured_snapshot(&snapshot) {
            Err(failure) => failure,
            Ok(_) => panic!("capture error unexpectedly built"),
        };
        assert_eq!(failure.envelope["diagnostics"][0]["rule"], "editor/input");
        assert!(
            failure.envelope["diagnostics"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("injected read denial"))
        );

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn unrelated_read_failures_remain_observable_without_rejecting_editor_model() {
        let root = corpus_root();
        let observed_root = project_scan_root(&root);
        let unrelated = observed_root.join("README.md");
        let snapshot = capture_project_snapshot_with(&root, &mut |path: &Path, _max_bytes| {
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
                .any(|failure| failure.path == unrelated && !failure.blocks_build)
        );
        build_captured_snapshot(&snapshot)
            .expect("unrelated failure must not reject the Editor model");
    }

    #[cfg(unix)]
    #[test]
    fn unknown_source_entry_failures_block_until_an_ordinary_file_kind_is_known() {
        use std::os::unix::fs::symlink;

        let root = temp_root("indeterminate-source-entry");
        copy_tree(&corpus_root(), &root);
        let observed_root = project_scan_root(&root);
        let unknown_source = observed_root.join("app/pending-save");
        let ordinary_source_file = observed_root.join("components/notes.txt");
        let unknown_root_file = observed_root.join("pending-save");

        assert!(project_indeterminate_path_blocks(
            &observed_root,
            &unknown_source,
            true,
            false,
        ));
        assert!(!project_path_blocks(
            &observed_root,
            &ordinary_source_file,
            true,
            false,
        ));
        assert!(!project_indeterminate_path_blocks(
            &observed_root,
            &unknown_root_file,
            false,
            false,
        ));

        symlink("missing-target", root.join("app/pending-save")).expect("dangling source symlink");
        let unknown_source_snapshot = capture_project_snapshot(&root);
        assert!(
            unknown_source_snapshot
                .failures
                .iter()
                .any(|failure| failure.path == unknown_source && failure.blocks_build)
        );
        assert!(build_captured_snapshot(&unknown_source_snapshot).is_err());
        std::fs::remove_file(root.join("app/pending-save")).expect("remove source symlink");

        std::fs::write(root.join("components/notes.txt"), "designer notes")
            .expect("ordinary source-adjacent file");
        let ordinary_file_snapshot =
            capture_project_snapshot_with(&root, &mut |path: &Path, _max_bytes| {
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
                .any(|failure| failure.path == ordinary_source_file && !failure.blocks_build)
        );
        build_captured_snapshot(&ordinary_file_snapshot)
            .expect("known ordinary file failure must not reject the Editor model");

        let ignored_link = root.join("components/node_modules");
        let observed_ignored_link = observed_root.join("components/node_modules");
        symlink("missing-target", &ignored_link).expect("dangling ignored symlink");
        let ignored_link_snapshot = capture_project_snapshot(&root);
        assert!(
            ignored_link_snapshot
                .failures
                .iter()
                .any(|failure| failure.path == observed_ignored_link && !failure.blocks_build)
        );
        build_captured_snapshot(&ignored_link_snapshot)
            .expect("indeterminate ignored-name entry must not reject the Editor model");

        symlink("missing-target", root.join("pending-save")).expect("dangling unrelated symlink");
        let unknown_root_snapshot = capture_project_snapshot(&root);
        assert!(
            unknown_root_snapshot
                .failures
                .iter()
                .any(|failure| failure.path == unknown_root_file && !failure.blocks_build)
        );
        build_captured_snapshot(&unknown_root_snapshot)
            .expect("unrelated indeterminate entry must not reject the Editor model");

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

        let snapshot = capture_project_snapshot(&root);
        let observed_root = project_scan_root(&root);
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
        let missing = capture_project_snapshot(&root);
        assert_ne!(snapshot.fingerprint, missing.fingerprint);
        assert!(
            missing
                .fingerprint
                .contains_key(&observed_root.join("build/base.toml"))
        );
        assert!(build_captured_snapshot(&missing).is_err());

        std::fs::copy(root.join("catalog/base.toml"), root.join("build/base.toml"))
            .expect("restore catalog");
        let recovered = capture_project_snapshot(&root);
        assert_ne!(missing.fingerprint, recovered.fingerprint);
        build_captured_snapshot(&recovered).expect("restored generated dependency recovers");

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn safe_symlinked_dependencies_build_while_cycles_and_escapes_reject_and_recover() {
        use std::os::unix::fs::symlink;

        let root = temp_root("editor-model-symlinks");
        copy_tree(&corpus_root(), &root);
        let expected = build_captured_snapshot(&capture_project_snapshot(&root))
            .expect("baseline")
            .render
            .to_json();
        let original_manifest = std::fs::read_to_string(root.join("uhura.toml")).expect("manifest");

        symlink("catalog/base.toml", root.join("catalog-link.toml")).expect("safe file symlink");
        let linked_file_manifest = original_manifest.replace(
            "path = \"catalog/base.toml\"",
            "path = \"catalog-link.toml\"",
        );
        std::fs::write(root.join("uhura.toml"), &linked_file_manifest).expect("file-link manifest");
        let linked_file = capture_project_snapshot(&root);
        assert!(
            linked_file
                .fingerprint
                .get(&project_scan_root(&root).join("catalog-link.toml"))
                .is_some_and(|identity| identity.starts_with("!symlink-file:"))
        );
        assert_eq!(
            build_captured_snapshot(&linked_file)
                .expect("safe file link builds")
                .render
                .to_json(),
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
        let linked_dir = capture_project_snapshot(&root);
        assert!(
            linked_dir
                .fingerprint
                .get(&project_scan_root(&root).join("catalog-link"))
                .is_some_and(|identity| identity.starts_with("!symlink-directory:"))
        );
        assert_eq!(
            build_captured_snapshot(&linked_dir)
                .expect("safe directory link builds")
                .render
                .to_json(),
            expected
        );

        symlink(root.join("components"), root.join("components/cycle")).expect("source cycle");
        let cycle = capture_project_snapshot(&root);
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
        let escaped = capture_project_snapshot(&root);
        assert!(build_captured_snapshot(&escaped).is_err());

        std::fs::remove_file(root.join("catalog-link.toml")).expect("remove escape");
        symlink("catalog/base.toml", root.join("catalog-link.toml")).expect("restore safe link");
        let recovered = capture_project_snapshot(&root);
        assert_ne!(escaped.fingerprint, recovered.fingerprint);
        assert_eq!(
            build_captured_snapshot(&recovered)
                .expect("safe retarget recovers")
                .render
                .to_json(),
            expected
        );

        std::fs::remove_file(outside).expect("outside cleanup");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
