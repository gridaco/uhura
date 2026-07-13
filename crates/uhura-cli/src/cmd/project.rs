//! `uhura project [path] [--out=<dir>]` — checked program → resolved
//! example previews (pinned + replay-derived, §6.2) → `eval_view` → V →
//! HTML → one self-contained `renders/canvas.html` (§8.3). Zero
//! transitions, commands, network — replay already happened in the check.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use uhura_base::{Severity, render_text};
use uhura_check::check;
use uhura_check::preview::PreviewPayload;
use uhura_check::resolve::SubjectKind;
use uhura_core::eval::{eval_fragment, eval_view};
use uhura_project::{Asset, FrameContent, FrameKind, PreviewFrame, render_canvas};

use crate::CommonArgs;

pub fn run(common: &CommonArgs, out_dir: Option<&str>) -> ExitCode {
    run_as(common, out_dir, "uhura project")
}

/// Shared Canvas build path for the build-only compatibility command and the
/// read-only editor host. The command label keeps diagnostics honest about the
/// entry point the user actually invoked.
pub(crate) fn run_as(common: &CommonArgs, out_dir: Option<&str>, command: &str) -> ExitCode {
    let root = &common.root;
    let input = match super::assemble_input(root) {
        Ok(input) => input,
        Err(code) => return code,
    };
    let output = check(&input);
    if output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        print!("{}", render_text(&output.diagnostics, &output.source_map));
        eprintln!("{command}: the check must come up clean first");
        return ExitCode::from(1);
    }
    let Some(lowered) = &output.lowered else {
        eprintln!("{command}: no checked program");
        return ExitCode::from(1);
    };
    let program = &lowered.program;

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
                    eprintln!("{command}: {subject}/{}: {e}", preview.example);
                    return ExitCode::from(1);
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
                    eprintln!("{command}: no definition `{name}`");
                    return ExitCode::from(1);
                };
                match eval_fragment(program, def, props, state, x) {
                    Ok(node) => FrameContent::Fragment(node),
                    Err(e) => {
                        eprintln!("{command}: {subject}/{}: {e}", preview.example);
                        return ExitCode::from(1);
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
            content,
        });
    }

    // ── assets: manifest + JPEG bytes → data URIs ──────────────────────
    let assets = match load_assets(root, input.manifest.assets_manifest.as_deref()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{command}: assets: {e}");
            return ExitCode::from(2);
        }
    };

    let html = render_canvas(
        input.manifest.app_name.as_str(),
        &frames,
        &output.stylesheet,
        &assets,
    );

    let out_dir = out_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("renders"));
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("{command}: {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }
    let out_path = out_dir.join("canvas.html");
    if let Err(e) = std::fs::write(&out_path, &html) {
        eprintln!("{command}: {}: {e}", out_path.display());
        return ExitCode::from(2);
    }
    println!(
        "{command}: projected {} previews ({} replay-derived) → {} ({} KiB)",
        frames.len(),
        derived,
        out_path.display(),
        html.len() / 1024
    );
    ExitCode::SUCCESS
}

pub(crate) fn output_path(out_dir: Option<&str>) -> PathBuf {
    out_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("renders"))
        .join("canvas.html")
}

fn load_assets(
    root: &Path,
    manifest_rel: Option<&str>,
) -> Result<std::collections::BTreeMap<String, Asset>, String> {
    let mut out = std::collections::BTreeMap::new();
    let Some(manifest_rel) = manifest_rel else {
        return Ok(out);
    };
    let manifest_path = root.join(manifest_rel);
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let table: toml::Table = text.parse().map_err(|e| format!("manifest: {e}"))?;
    let Some(assets) = table.get("assets").and_then(toml::Value::as_table) else {
        return Ok(out);
    };
    let asset_dir = manifest_path.parent().unwrap_or(root);
    for (id, entry) in assets {
        let file = entry.get("file").and_then(toml::Value::as_str);
        let alt = entry.get("alt").and_then(toml::Value::as_str);
        let (Some(file), Some(alt)) = (file, alt) else {
            return Err(format!("asset `{id}` needs `file` and `alt` (§8.3)"));
        };
        // Missing files fall back to the duotone SVG at render time.
        if let Ok(bytes) = std::fs::read(asset_dir.join(file)) {
            out.insert(
                id.clone(),
                Asset {
                    data_uri: format!("data:image/jpeg;base64,{}", base64(&bytes)),
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
