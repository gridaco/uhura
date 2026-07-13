//! The `uhura` CLI: check | fmt | project | dev | trace. Thin argument
//! parsing over the library crate (`uhura_cli::cmd`) — the same code the
//! gate tests drive.

use std::path::PathBuf;
use std::process::ExitCode;

use uhura_cli::{CommonArgs, cmd};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("usage: uhura <check|fmt|project|dev|trace> [path] [flags]");
        return ExitCode::from(2);
    };

    let mut root = PathBuf::from(".");
    let mut format_json = false;
    let mut deny_warnings = false;
    let mut check_only = false;
    let mut emit_ir = false;
    let mut out_dir: Option<String> = None;
    let mut script: Option<String> = None;
    let mut expanded = false;
    let mut port: u16 = 8787;

    let mut args = args.peekable();
    while let Some(a) = args.next() {
        match a.as_str() {
            // Space-separated `--format <value>` consumes its value here —
            // it must never fall through to the positional path.
            "--format" => match args.next().as_deref() {
                Some("json") => format_json = true,
                Some("text") => format_json = false,
                other => {
                    eprintln!(
                        "--format takes `json` or `text`, got {}",
                        other.unwrap_or("nothing")
                    );
                    return ExitCode::from(2);
                }
            },
            "--format=json" | "--json" => format_json = true,
            "--format=text" => format_json = false,
            "--deny-warnings" => deny_warnings = true,
            "--check" => check_only = true,
            "--emit-ir" => emit_ir = true,
            other if other.starts_with("--out=") => {
                out_dir = Some(other["--out=".len()..].to_string());
            }
            other if other.starts_with("--script=") => {
                script = Some(other["--script=".len()..].to_string());
            }
            "--expanded" => expanded = true,
            // Space-separated `--port <n>` consumes its value, like --format.
            "--port" => match args.next().as_deref().map(str::parse) {
                Some(Ok(p)) => port = p,
                _ => {
                    eprintln!("--port takes a port number");
                    return ExitCode::from(2);
                }
            },
            other if other.starts_with("--port=") => match other["--port=".len()..].parse() {
                Ok(p) => port = p,
                Err(_) => {
                    eprintln!("--port takes a port number");
                    return ExitCode::from(2);
                }
            },
            other if !other.starts_with('-') => {
                root = PathBuf::from(other);
            }
            other => {
                eprintln!("unknown flag: {other}");
                return ExitCode::from(2);
            }
        }
    }

    let common = CommonArgs {
        root,
        format_json,
        deny_warnings,
        emit_ir,
    };
    match cmd.as_str() {
        "fmt" => cmd::fmt::run(&common, check_only),
        "check" => cmd::check::run(&common),
        "project" => cmd::project::run(&common, out_dir.as_deref()),
        "trace" => cmd::trace::run(&common, script.as_deref(), expanded),
        "dev" => cmd::dev::run(&common, port),
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: uhura <check|fmt|project|dev|trace> [path] [flags]");
            ExitCode::from(2)
        }
    }
}
