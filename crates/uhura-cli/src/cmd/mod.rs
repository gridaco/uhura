pub mod check;
pub mod dev;
pub mod editor;
pub mod fmt;
pub mod graph;
pub mod trace;

use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use uhura_check::icon_fonts::{MAX_ICON_FONT_BYTES, MAX_ICON_GLYPH_MAP_BYTES};
use uhura_check::manifest::load_manifest;
use uhura_check::{CheckInput, IconFontInput, SourceInput};

use crate::fsio::walk_corpus;

/// Reads everything the Uhura checker needs from a corpus root
/// and assembles the pure pipeline input.
pub fn assemble_input(root: &Path) -> Result<CheckInput, ExitCode> {
    // ── manifest (tells us what else to read) ──────────────────────────
    let manifest_path = root.join("uhura.toml");
    let manifest_text = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "uhura check: {}: {e}\n(a corpus root needs a `uhura.toml` manifest — design §3)",
                manifest_path.display()
            );
            return Err(ExitCode::from(2));
        }
    };
    let manifest = match load_manifest(&manifest_text) {
        Ok(m) => m,
        Err(issues) => {
            for issue in &issues {
                eprintln!("uhura.toml: {}: {}", issue.path, issue.message);
            }
            return Err(ExitCode::from(1));
        }
    };

    // ── referenced files ───────────────────────────────────────────────
    let read_opt = |rel: &str| std::fs::read_to_string(root.join(rel)).ok();
    let catalog_file = (
        manifest.catalog_path.clone(),
        read_opt(&manifest.catalog_path),
    );
    let canonical_root = std::fs::canonicalize(root).ok();
    let read_icon_bytes = |rel: &str, max_bytes: usize| {
        let root = canonical_root.as_ref()?;
        let path = std::fs::canonicalize(root.join(rel)).ok()?;
        path.starts_with(root)
            .then(|| read_bounded_file(&path, max_bytes).ok())
            .flatten()
    };
    let icon_font_files = manifest
        .icons
        .families
        .iter()
        .map(|(name, family)| {
            (
                name.clone(),
                IconFontInput {
                    font_path: family.font.clone(),
                    font_bytes: read_icon_bytes(&family.font, MAX_ICON_FONT_BYTES)
                        .map(Arc::<[u8]>::from),
                    glyphs_path: family.glyphs.clone(),
                    glyphs_text: read_icon_bytes(&family.glyphs, MAX_ICON_GLYPH_MAP_BYTES)
                        .and_then(|bytes| String::from_utf8(bytes).ok()),
                },
            )
        })
        .collect();
    let port_files = manifest
        .ports
        .iter()
        .map(|(name, rel)| (name.clone(), (rel.clone(), read_opt(rel))))
        .collect();
    let theme_css = read_opt("styles/theme.css").map(|css| ("styles/theme.css".to_string(), css));
    let fixture_files = manifest
        .fixtures
        .iter()
        .map(|(name, rel)| (name.clone(), (rel.clone(), read_opt(rel))))
        .collect();
    let lock_text = read_opt("uhura.lock");

    let files = match walk_corpus(root) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("uhura check: {}: {e}", root.display());
            return Err(ExitCode::from(2));
        }
    };
    if files.is_empty() {
        eprintln!("uhura check: no .uhura sources under {}", root.display());
        return Err(ExitCode::from(2));
    }
    let sources = files
        .into_iter()
        .map(|f| SourceInput {
            rel_path: f.rel_path,
            text: f.text,
            kind: f.kind,
        })
        .collect();

    // ── the pure pipeline ──────────────────────────────────────────────
    let input = CheckInput {
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
    };
    Ok(input)
}

fn read_bounded_file(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > max_bytes as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file exceeds {max_bytes}-byte limit"),
        ));
    }

    // The handle-level bound closes the metadata/read race if a file grows
    // after the size check.
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::read_bounded_file;

    #[test]
    fn bounded_resource_read_rejects_size_before_loading_the_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "uhura-bounded-icon-read-{}-{unique}",
            std::process::id()
        ));
        let file = std::fs::File::create(&path).expect("create sparse file");
        file.set_len(9).expect("size sparse file");

        let error = read_bounded_file(&path, 8).expect_err("oversized file must be rejected");
        assert!(error.to_string().contains("8-byte limit"));

        std::fs::remove_file(path).expect("cleanup");
    }
}
