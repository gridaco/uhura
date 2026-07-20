//! Deterministic filesystem discovery for canonical Uhura source modules.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct SourceFile {
    /// Project-relative path with `/` separators.
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub text: String,
}

/// Read every `.uhura` module in the same project namespace observed by the
/// host. Generated output roots and dependency metadata are never source.
pub fn walk_sources(root: &Path) -> io::Result<Vec<SourceFile>> {
    let canonical_root = std::fs::canonicalize(root)?;
    let mut sources = Vec::new();
    let mut active = BTreeSet::from([canonical_root.clone()]);
    walk_directory(
        &canonical_root,
        &canonical_root,
        Path::new(""),
        &mut active,
        &mut |logical, actual| {
            if logical.extension().and_then(|extension| extension.to_str()) != Some("uhura") {
                return Ok(());
            }
            sources.push(SourceFile {
                rel_path: portable_path(logical),
                abs_path: actual.to_path_buf(),
                text: std::fs::read_to_string(actual)?,
            });
            Ok(())
        },
    )?;
    sources.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
    Ok(sources)
}

pub fn walk_retired_sources(root: &Path) -> io::Result<Vec<PathBuf>> {
    let canonical_root = std::fs::canonicalize(root)?;
    let mut paths = Vec::new();
    let mut active = BTreeSet::from([canonical_root.clone()]);
    walk_directory(
        &canonical_root,
        &canonical_root,
        Path::new(""),
        &mut active,
        &mut |logical, _| {
            if logical.extension().and_then(|extension| extension.to_str()) == Some("relay") {
                paths.push(logical.to_path_buf());
            }
            Ok(())
        },
    )?;
    paths.sort();
    Ok(paths)
}

fn walk_directory(
    root: &Path,
    actual: &Path,
    logical: &Path,
    active: &mut BTreeSet<PathBuf>,
    visit: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut entries = std::fs::read_dir(actual)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let logical_path = logical.join(entry.file_name());
        if ignored(&logical_path) {
            continue;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            let target = std::fs::canonicalize(&path)?;
            if !target.starts_with(root) {
                if is_source_path(&logical_path) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!(
                            "source symlink escapes the project: {}",
                            logical_path.display()
                        ),
                    ));
                }
                continue;
            }
            let target_metadata = std::fs::metadata(&target)?;
            if target_metadata.is_dir() {
                if !active.insert(target.clone()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("directory cycle: {}", logical_path.display()),
                    ));
                }
                walk_directory(root, &target, &logical_path, active, visit)?;
                active.remove(&target);
            } else if target_metadata.is_file() {
                visit(&logical_path, &target)?;
            }
            continue;
        }
        if metadata.is_dir() {
            let canonical = std::fs::canonicalize(&path)?;
            if !active.insert(canonical.clone()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("directory cycle: {}", logical_path.display()),
                ));
            }
            walk_directory(root, &canonical, &logical_path, active, visit)?;
            active.remove(&canonical);
        } else if metadata.is_file() {
            visit(&logical_path, &path)?;
        }
    }
    Ok(())
}

fn is_source_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("uhura" | "relay")
    )
}

fn ignored(path: &Path) -> bool {
    let root = path
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str());
    if matches!(root, Some("build" | "renders" | "target")) {
        return true;
    }
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(".git" | "node_modules")
        )
    })
}

fn portable_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_project_wide_sources_but_not_generated_output() {
        let root =
            std::env::temp_dir().join(format!("uhura-cli-source-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("feature")).unwrap();
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::write(root.join("machine.uhura"), "root").unwrap();
        std::fs::write(root.join("feature/ui.uhura"), "nested").unwrap();
        std::fs::write(root.join("build/stale.uhura"), "ignored").unwrap();

        let sources = walk_sources(&root).unwrap();
        assert_eq!(
            sources
                .iter()
                .map(|source| source.rel_path.as_str())
                .collect::<Vec<_>>(),
            ["feature/ui.uhura", "machine.uhura"]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
