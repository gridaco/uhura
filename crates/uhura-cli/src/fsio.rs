//! Filesystem walking — the only place in the workspace that reads source
//! trees (design §12.1). Produces the in-memory `SourceSet` every pure crate
//! consumes.

use std::path::{Path, PathBuf};

use uhura_syntax::SourceKind;

pub struct SourceFile {
    /// Corpus-relative path with `/` separators (deterministic across OSes).
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub text: String,
    pub kind: SourceKind,
}

/// Walks a corpus root for `.uhura` sources, sorted by relative path.
pub fn walk_corpus(root: &Path) -> std::io::Result<Vec<SourceFile>> {
    let mut out = Vec::new();
    for dir in ["app", "components", "surfaces"] {
        let base = root.join(dir);
        if base.is_dir() {
            walk_dir(root, &base, &mut out)?;
        }
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<SourceFile>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir(root, &path, out)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".uhura") {
            continue;
        }
        let kind = if name.ends_with(".examples.uhura") {
            SourceKind::Examples
        } else {
            SourceKind::Module
        };
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let text = std::fs::read_to_string(&path)?;
        out.push(SourceFile {
            rel_path,
            abs_path: path,
            text,
            kind,
        });
    }
    Ok(())
}
