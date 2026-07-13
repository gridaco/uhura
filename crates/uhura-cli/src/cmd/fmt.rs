//! `uhura fmt [path] [--check]` — canonical-formats every `.uhura` source.
//! `--check` reports drift without writing (CI mode).

use std::process::ExitCode;

use uhura_base::{SourceMap, has_errors, render_text};
use uhura_syntax::{Parsed, parse};

use crate::CommonArgs;
use crate::fsio::walk_corpus;

pub fn run(common: &CommonArgs, check_only: bool) -> ExitCode {
    let files = match walk_corpus(&common.root) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("uhura fmt: {}: {e}", common.root.display());
            return ExitCode::from(2);
        }
    };
    if files.is_empty() {
        eprintln!(
            "uhura fmt: no .uhura sources under {}",
            common.root.display()
        );
        return ExitCode::from(2);
    }

    let mut sm = SourceMap::new();
    let mut drifted = Vec::new();
    let mut broken = false;

    for f in &files {
        let id = sm.add(f.rel_path.clone(), f.text.clone());
        let out = parse(id, &f.text, f.kind);
        if has_errors(&out.diagnostics) {
            // Refuse to format files that do not parse.
            eprint!("{}", render_text(&out.diagnostics, &sm));
            broken = true;
            continue;
        }
        let formatted = match &out.parsed {
            Parsed::Module(m) => uhura_syntax::format_module(m),
            Parsed::Examples(e) => uhura_syntax::format_examples(e),
        };
        if formatted != f.text {
            drifted.push(f.rel_path.clone());
            if !check_only && let Err(e) = std::fs::write(&f.abs_path, &formatted) {
                eprintln!("uhura fmt: cannot write {}: {e}", f.rel_path);
                return ExitCode::from(2);
            }
        }
    }

    if broken {
        return ExitCode::from(1);
    }
    if check_only && !drifted.is_empty() {
        for p in &drifted {
            println!("would reformat: {p}");
        }
        return ExitCode::from(1);
    }
    if !check_only {
        for p in &drifted {
            println!("reformatted: {p}");
        }
    }
    ExitCode::SUCCESS
}
