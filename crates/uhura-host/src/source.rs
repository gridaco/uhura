//! Coherent Uhura project capture.
//!
//! The host observes one immutable set of project-relative bytes and gives
//! that same revision to checking, evidence replay, Editor publication, and
//! Play admission. Browser code never re-reads the project behind that
//! revision.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectSourceFiles {
    entries: BTreeMap<PathBuf, Arc<[u8]>>,
    /// Canonical on-disk identity for detecting two logical source paths that
    /// alias one physical file.
    origins: BTreeMap<PathBuf, PathBuf>,
    case_insensitive: bool,
}

impl ProjectSourceFiles {
    fn insert(&mut self, path: PathBuf, origin: PathBuf, bytes: Arc<[u8]>) {
        self.origins.insert(path.clone(), origin);
        self.entries.insert(path, bytes);
    }

    pub(crate) fn resolve(&self, path: &Path) -> Result<Option<&Arc<[u8]>>, String> {
        let path = normalize_project_path(path)?;
        if let Some(bytes) = self.entries.get(&path) {
            return Ok(Some(bytes));
        }
        if !self.case_insensitive {
            return Ok(None);
        }

        let Some(wanted) = case_key(&path) else {
            return Ok(None);
        };
        let matches = self
            .entries
            .iter()
            .filter(|(candidate, _)| case_key(candidate).as_ref() == Some(&wanted))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Ok(None),
            [(_, bytes)] => Ok(Some(bytes)),
            _ => Err(format!(
                "{} is ambiguous in the captured project",
                path.display()
            )),
        }
    }

    pub(crate) fn text(&self, relative: &str) -> Result<Option<String>, String> {
        self.resolve(Path::new(relative))?
            .map(|bytes| {
                std::str::from_utf8(bytes)
                    .map(str::to_owned)
                    .map_err(|error| format!("{relative}: source is not UTF-8: {error}"))
            })
            .transpose()
    }

    pub(crate) fn sources(&self) -> Result<Vec<(String, String)>, String> {
        let mut sources = self
            .entries
            .iter()
            .filter(|(path, _)| {
                path.extension().and_then(|extension| extension.to_str()) == Some("uhura")
            })
            .map(|(path, bytes)| {
                let text = std::str::from_utf8(bytes)
                    .map_err(|error| format!("{}: source is not UTF-8: {error}", path.display()))?;
                Ok((portable_path(path), text.to_owned()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        sources.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(sources)
    }

    pub(crate) fn retired_relay_sources(&self) -> Vec<String> {
        self.entries
            .keys()
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("relay")
            })
            .map(|path| portable_path(path))
            .collect()
    }

    pub(crate) fn duplicate_sources(&self) -> Vec<Vec<String>> {
        let mut by_origin = BTreeMap::<&Path, Vec<String>>::new();
        for (logical, origin) in &self.origins {
            if logical.extension().and_then(|extension| extension.to_str()) != Some("uhura") {
                continue;
            }
            by_origin
                .entry(origin.as_path())
                .or_default()
                .push(portable_path(logical));
        }
        by_origin
            .into_values()
            .filter_map(|mut paths| {
                paths.sort();
                paths.dedup();
                (paths.len() > 1).then_some(paths)
            })
            .collect()
    }

    pub(crate) fn subtree(&self, prefix: &Path) -> BTreeMap<String, Arc<[u8]>> {
        self.entries
            .iter()
            .filter_map(|(path, bytes)| {
                let relative = path.strip_prefix(prefix).ok()?;
                (!relative.as_os_str().is_empty())
                    .then(|| (portable_path(relative), Arc::clone(bytes)))
            })
            .collect()
    }
}

pub(crate) fn normalize_project_path(path: &Path) -> Result<PathBuf, String> {
    let display = path.display().to_string();
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(format!("{display}: path escapes the project root"));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("{display}: expected a project-relative path"));
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

fn portable_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
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
    /// Deterministic identity for the complete observation.
    #[must_use]
    pub fn stable_id(&self) -> String {
        let mut bytes = b"uhura-project-source-fingerprint/1\0".to_vec();
        bytes.push(u8::from(self.case_insensitive));
        bytes.extend_from_slice(&(self.entries.len() as u64).to_be_bytes());
        for (path, value) in &self.entries {
            append_field(&mut bytes, portable_path(path).as_bytes());
            append_field(&mut bytes, value.as_bytes());
        }
        uhura_base::sha256_hex(&bytes)
    }
}

fn append_field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
    bytes.extend_from_slice(field);
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProjectCaptureFailure {
    path: PathBuf,
    operation: &'static str,
    message: String,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectSourceSnapshot {
    pub(crate) files: ProjectSourceFiles,
    pub(crate) fingerprint: ProjectSourceFingerprint,
    source_revision_id: String,
    failures: Vec<ProjectCaptureFailure>,
}

impl ProjectSourceSnapshot {
    /// Content identity of every captured project input.
    #[must_use]
    pub fn fingerprint(&self) -> &ProjectSourceFingerprint {
        &self.fingerprint
    }

    /// `uhura-source-revision/0` identity of the exact captured paths and raw
    /// bytes. Unlike the polling fingerprint, this is the persisted
    /// source/provenance identity defined by the 0.4 project contract.
    #[must_use]
    pub fn source_revision_id(&self) -> &str {
        &self.source_revision_id
    }

    pub(crate) fn has_retired_relay_corpus(&self) -> bool {
        !self.files.retired_relay_sources().is_empty()
            || self.failures.iter().any(|failure| {
                failure
                    .path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    == Some("relay")
            })
    }

    pub(crate) fn validate_for_build(&self) -> Result<(), serde_json::Value> {
        if self.failures.is_empty() {
            return Ok(());
        }
        let mut details = self
            .failures
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
        if self.failures.len() > details.len() {
            details.push(format!(
                "and {} more filesystem errors",
                self.failures.len() - details.len()
            ));
        }
        Err(serde_json::json!({
            "format": "uhura-diagnostics",
            "version": 0,
            "summary": { "errors": 1, "warnings": 0 },
            "diagnostics": [{
                "code": "UH9000",
                "rule": "uhura/source",
                "severity": "error",
                "message": format!(
                    "could not capture a complete project snapshot: {}",
                    details.join("; ")
                ),
            }],
        }))
    }
}

/// Capture every observable project input once.
pub fn capture_project_snapshot(root: &Path) -> ProjectSourceSnapshot {
    let canonical_root = match std::fs::canonicalize(root) {
        Ok(root) => root,
        Err(error) => {
            return ProjectSourceSnapshot {
                failures: vec![ProjectCaptureFailure {
                    path: root.to_path_buf(),
                    operation: "resolve project root",
                    message: error.to_string(),
                }],
                ..ProjectSourceSnapshot::default()
            };
        }
    };

    let case_insensitive = detect_case_insensitive_filesystem(&canonical_root);
    let mut snapshot = ProjectSourceSnapshot::default();
    snapshot.files.case_insensitive = case_insensitive;
    snapshot.fingerprint.case_insensitive = case_insensitive;
    let mut active_directories = BTreeSet::from([canonical_root.clone()]);
    capture_directory(
        &canonical_root,
        &canonical_root,
        Path::new(""),
        &mut snapshot,
        &mut active_directories,
    );
    snapshot.failures.sort();
    let files = snapshot
        .files
        .entries
        .iter()
        .map(|(path, bytes)| (portable_path(path), bytes.as_ref()))
        .collect::<Vec<_>>();
    snapshot.source_revision_id = uhura_core::source_revision_id(
        case_insensitive,
        files.iter().map(|(path, bytes)| (path.as_str(), *bytes)),
    )
    .expect("captured project paths are normalized and unique");
    snapshot
}

fn capture_directory(
    root: &Path,
    actual: &Path,
    logical: &Path,
    snapshot: &mut ProjectSourceSnapshot,
    active_directories: &mut BTreeSet<PathBuf>,
) {
    let entries = match std::fs::read_dir(actual) {
        Ok(entries) => entries,
        Err(error) => {
            capture_failure(snapshot, logical, "read directory", error);
            return;
        }
    };
    let entries = match entries.collect::<Result<Vec<_>, _>>() {
        Ok(mut entries) => {
            entries.sort_by_key(std::fs::DirEntry::file_name);
            entries
        }
        Err(error) => {
            capture_failure(snapshot, logical, "read directory entry", error);
            return;
        }
    };

    for entry in entries {
        let name = entry.file_name();
        let logical_path = logical.join(&name);
        if ignored_project_path(&logical_path) {
            continue;
        }
        let entry_path = entry.path();
        let metadata = match std::fs::symlink_metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                capture_failure(snapshot, &logical_path, "inspect file", error);
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            let target = match std::fs::canonicalize(&entry_path) {
                Ok(target) if target.starts_with(root) => target,
                Ok(_) => {
                    capture_failure(
                        snapshot,
                        &logical_path,
                        "resolve symlink",
                        std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "symlink escapes the project root",
                        ),
                    );
                    continue;
                }
                Err(error) => {
                    capture_failure(snapshot, &logical_path, "resolve symlink", error);
                    continue;
                }
            };
            let target_metadata = match std::fs::metadata(&target) {
                Ok(metadata) => metadata,
                Err(error) => {
                    capture_failure(snapshot, &logical_path, "inspect symlink target", error);
                    continue;
                }
            };
            if target_metadata.is_dir() {
                if !active_directories.insert(target.clone()) {
                    capture_failure(
                        snapshot,
                        &logical_path,
                        "traverse directory",
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "directory cycle"),
                    );
                    continue;
                }
                capture_directory(root, &target, &logical_path, snapshot, active_directories);
                active_directories.remove(&target);
            } else if target_metadata.is_file() {
                capture_file(&target, &logical_path, snapshot);
            }
            continue;
        }

        if metadata.is_dir() {
            let canonical = match std::fs::canonicalize(&entry_path) {
                Ok(canonical) => canonical,
                Err(error) => {
                    capture_failure(snapshot, &logical_path, "resolve directory", error);
                    continue;
                }
            };
            if !active_directories.insert(canonical.clone()) {
                capture_failure(
                    snapshot,
                    &logical_path,
                    "traverse directory",
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "directory cycle"),
                );
                continue;
            }
            capture_directory(
                root,
                &canonical,
                &logical_path,
                snapshot,
                active_directories,
            );
            active_directories.remove(&canonical);
        } else if metadata.is_file() {
            capture_file(&entry_path, &logical_path, snapshot);
        }
    }
}

fn capture_file(actual: &Path, logical: &Path, snapshot: &mut ProjectSourceSnapshot) {
    match std::fs::read(actual) {
        Ok(bytes) => {
            let origin = std::fs::canonicalize(actual).unwrap_or_else(|_| actual.to_path_buf());
            snapshot
                .fingerprint
                .insert(logical.to_path_buf(), uhura_base::sha256_hex(&bytes));
            snapshot
                .files
                .insert(logical.to_path_buf(), origin, Arc::from(bytes));
        }
        Err(error) => capture_failure(snapshot, logical, "read file", error),
    }
}

fn capture_failure(
    snapshot: &mut ProjectSourceSnapshot,
    path: &Path,
    operation: &'static str,
    error: std::io::Error,
) {
    snapshot.fingerprint.insert(
        path.to_path_buf(),
        format!("!error:{operation}:{:?}:{error}", error.kind()),
    );
    snapshot.failures.push(ProjectCaptureFailure {
        path: path.to_path_buf(),
        operation,
        message: error.to_string(),
    });
}

fn ignored_project_path(path: &Path) -> bool {
    let mut components = path.components();
    let root = components
        .next()
        .map(|component| component.as_os_str().to_string_lossy());
    if root
        .as_deref()
        .is_some_and(|name| ["build", "renders", "target"].contains(&name))
    {
        return true;
    }
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(".git" | "node_modules")
        )
    })
}

fn detect_case_insensitive_filesystem(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(alias) = toggled_ascii_case(name) else {
            continue;
        };
        if alias == name {
            continue;
        }
        let alias_path = root.join(alias);
        if !alias_path.exists() {
            continue;
        }
        if same_file::is_same_file(entry.path(), alias_path).unwrap_or(false) {
            return true;
        }
    }
    false
}

fn toggled_ascii_case(name: &str) -> Option<String> {
    let (index, character) = name
        .char_indices()
        .find(|(_, character)| character.is_ascii_alphabetic())?;
    let mut alias = name.to_owned();
    let replacement = if character.is_ascii_lowercase() {
        character.to_ascii_uppercase()
    } else {
        character.to_ascii_lowercase()
    };
    alias.replace_range(
        index..index + character.len_utf8(),
        &replacement.to_string(),
    );
    Some(alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_paths_cannot_escape_the_snapshot() {
        assert!(normalize_project_path(Path::new("../outside")).is_err());
        assert!(normalize_project_path(Path::new("/outside")).is_err());
        assert_eq!(
            normalize_project_path(Path::new("./machine.uhura")).unwrap(),
            PathBuf::from("machine.uhura")
        );
    }

    #[test]
    fn fingerprint_is_order_independent_and_content_sensitive() {
        let mut first = ProjectSourceFingerprint::default();
        first.entries.insert(PathBuf::from("b"), "two".to_owned());
        first.entries.insert(PathBuf::from("a"), "one".to_owned());
        let mut second = ProjectSourceFingerprint::default();
        second.entries.insert(PathBuf::from("a"), "one".to_owned());
        second.entries.insert(PathBuf::from("b"), "two".to_owned());
        assert_eq!(first.stable_id(), second.stable_id());
        second
            .entries
            .insert(PathBuf::from("b"), "changed".to_owned());
        assert_ne!(first.stable_id(), second.stable_id());
    }

    #[test]
    fn capture_ignores_generated_roots_and_keeps_uhura_sources() {
        let root =
            std::env::temp_dir().join(format!("uhura-source-capture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::write(root.join("machine.uhura"), "language uhura 0.3\n").unwrap();
        std::fs::write(root.join("build/stale.uhura"), "invalid").unwrap();

        let snapshot = capture_project_snapshot(&root);
        assert!(snapshot.validate_for_build().is_ok());
        assert_eq!(snapshot.files.sources().unwrap().len(), 1);
        assert_eq!(
            snapshot.source_revision_id(),
            uhura_core::source_revision_id(
                snapshot.files.case_insensitive,
                [("machine.uhura", b"language uhura 0.3\n".as_slice())],
            )
            .unwrap()
        );
        assert!(
            snapshot
                .files
                .resolve(Path::new("build/stale.uhura"))
                .unwrap()
                .is_none()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn capture_retains_physical_identity_for_source_alias_rejection() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("uhura-source-alias-capture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("machine.uhura"), "machine").unwrap();
        symlink("machine.uhura", root.join("alias.uhura")).unwrap();

        let snapshot = capture_project_snapshot(&root);
        assert_eq!(
            snapshot.files.duplicate_sources(),
            vec![vec!["alias.uhura".to_string(), "machine.uhura".to_string()]]
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
