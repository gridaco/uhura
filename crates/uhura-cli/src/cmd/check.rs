//! `uhura check [path] [--emit-ir] [--deny-warnings] [--format=json]` —
//! the full pipeline (§12.2): manifest → catalog/ports → parse → resolve →
//! typecheck → markup → style → examples → lower. This file does every
//! read and write; the pipeline itself is pure.

use std::path::Path;
use std::process::ExitCode;

use uhura_base::{Severity, render_text, to_envelope};
use uhura_check::{LockStatus, check};

use crate::CommonArgs;

pub fn run(common: &CommonArgs) -> ExitCode {
    let root = &common.root;

    let input = match super::assemble_input(root) {
        Ok(input) => input,
        Err(code) => return code,
    };
    let output = check(&input);

    // ── lock: write when absent (micro-decision #6) ────────────────────
    if output.lock_status == LockStatus::Absent
        && !uhura_base::has_errors(&output.diagnostics)
        && let Err(e) = std::fs::write(root.join("uhura.lock"), &output.lock_computed)
    {
        eprintln!("uhura check: could not write uhura.lock: {e}");
        return ExitCode::from(2);
    }

    // ── --emit-ir ──────────────────────────────────────────────────────
    if common.emit_ir {
        match &output.lowered {
            None => {
                eprintln!("uhura check: not emitting IR — the check did not come up clean");
            }
            Some(lowered) => {
                if let Err(e) = emit_build_artifacts(root, lowered, &output.stylesheet) {
                    eprintln!("uhura check: emit-ir failed: {e}");
                    return ExitCode::from(2);
                }
            }
        }
    }

    // ── report ─────────────────────────────────────────────────────────
    let diags = &output.diagnostics;
    if common.format_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&to_envelope(diags, &output.source_map))
                .expect("envelope json")
        );
    } else if !diags.is_empty() {
        print!("{}", render_text(diags, &output.source_map));
    } else {
        println!("checked {} files: clean", input.sources.len());
    }

    let failing = diags.iter().any(|d| {
        d.severity == Severity::Error || (common.deny_warnings && d.severity == Severity::Warning)
    });
    if failing {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `build/ir.json` (canonical bytes — the golden artifact), `build/
/// ir-spans.json` (side table), `build/stylesheet.css` (compiled).
fn emit_build_artifacts(
    root: &Path,
    lowered: &uhura_check::lower::Lowered,
    stylesheet: &str,
) -> std::io::Result<()> {
    let build = root.join("build");
    std::fs::create_dir_all(&build)?;
    std::fs::write(build.join("ir.json"), lowered.program.to_canonical_string())?;
    let spans = serde_json::to_value(&lowered.spans).expect("span table is always serializable");
    std::fs::write(
        build.join("ir-spans.json"),
        uhura_base::to_canonical_json(&spans),
    )?;
    std::fs::write(build.join("stylesheet.css"), stylesheet)?;
    println!(
        "emitted build/ir.json ({} bytes, hash {}), build/ir-spans.json, build/stylesheet.css",
        lowered.program.to_canonical_string().len(),
        lowered.program.hash()
    );
    Ok(())
}
