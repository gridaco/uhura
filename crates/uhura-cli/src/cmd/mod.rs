pub mod check;
pub mod dev;
pub mod editor;
pub mod fmt;
pub mod project;
pub mod trace;

use std::path::Path;
use std::process::ExitCode;

use uhura_check::manifest::load_manifest;
use uhura_check::{CheckInput, SourceInput};

use crate::fsio::walk_corpus;

/// Reads everything `uhura check`/`uhura project` need from a corpus root
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
        port_files,
        sources,
        theme_css,
        fixture_files,
        lock_text,
    };
    Ok(input)
}
