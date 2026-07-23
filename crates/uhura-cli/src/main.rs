//! The `uhura` CLI: check | export | fmt | editor | play | trace | graph. With
//! no command it opens the read-only editor for the current directory. Thin
//! argument parsing over the library crate (`uhura_cli::cmd`) — the same code
//! the gate tests drive.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use uhura_cli::{CommonArgs, cmd};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CliCommand {
    Check,
    Export,
    Fmt,
    Editor,
    Play,
    Trace,
    Graph,
}

impl CliCommand {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "check" => Some(Self::Check),
            "export" => Some(Self::Export),
            "fmt" => Some(Self::Fmt),
            "editor" => Some(Self::Editor),
            "play" => Some(Self::Play),
            "trace" => Some(Self::Trace),
            "graph" => Some(Self::Graph),
            _ => None,
        }
    }
}

/// Resolve only the leading token. Known command names win; flags and
/// path-shaped values stay in the iterator as arguments to the default editor.
fn select_command(first: Option<&str>) -> Result<(CliCommand, bool), &str> {
    let Some(first) = first else {
        return Ok((CliCommand::Editor, false));
    };
    if let Some(command) = CliCommand::parse(first) {
        return Ok((command, true));
    }
    if first.starts_with('-') || looks_like_project_path(first) {
        return Ok((CliCommand::Editor, false));
    }
    Err(first)
}

fn looks_like_project_path(value: &str) -> bool {
    let path = Path::new(value);
    path.exists()
        || path.is_absolute()
        || value == "."
        || value == ".."
        || value.contains(['/', '\\'])
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1).peekable();
    let (command, consume_command) = match select_command(args.peek().map(String::as_str)) {
        Ok(selection) => selection,
        Err(name) => {
            eprintln!("unknown command: {name}");
            print_usage();
            return ExitCode::from(2);
        }
    };
    if consume_command {
        args.next();
    }

    let mut root = PathBuf::from(".");
    let mut format_json = false;
    let mut deny_warnings = false;
    let mut check_only = false;
    let mut emit_ir = false;
    let mut script: Option<String> = None;
    let mut out: Option<String> = None;
    let mut mount: Option<String> = None;
    let mut play_entry: Option<String> = None;
    let mut expanded = false;
    let mut port: u16 = 8787;

    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
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
            other if other.starts_with("--script=") => {
                script = Some(other["--script=".len()..].to_string());
            }
            other if other.starts_with("--out=") => {
                out = Some(other["--out=".len()..].to_string());
            }
            other if other.starts_with("--mount=") => {
                mount = Some(other["--mount=".len()..].to_string());
            }
            "--mount" => match args.next() {
                Some(v) => mount = Some(v),
                None => {
                    eprintln!("--mount takes an origin-local path");
                    return ExitCode::from(2);
                }
            },
            other if other.starts_with("--play-entry=") => {
                play_entry = Some(other["--play-entry=".len()..].to_string());
            }
            "--play-entry" => match args.next() {
                Some(v) => play_entry = Some(v),
                None => {
                    eprintln!("--play-entry takes an origin-local path");
                    return ExitCode::from(2);
                }
            },
            // Space-separated `--out <file>` consumes its value, like --format.
            "--out" => match args.next() {
                Some(v) => out = Some(v),
                None => {
                    eprintln!("--out takes a file path");
                    return ExitCode::from(2);
                }
            },
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
    if command != CliCommand::Export && (mount.is_some() || play_entry.is_some()) {
        eprintln!("--mount and --play-entry are valid only for `uhura export`");
        return ExitCode::from(2);
    }
    match command {
        CliCommand::Fmt => cmd::fmt::run(&common, check_only),
        CliCommand::Check => cmd::check::run(&common),
        CliCommand::Export => cmd::export::run(
            &common,
            out.as_deref(),
            mount.as_deref(),
            play_entry.as_deref(),
        ),
        CliCommand::Editor => cmd::editor::run(&common, port),
        CliCommand::Trace => cmd::trace::run(&common, script.as_deref(), expanded),
        CliCommand::Play => cmd::play::run(&common, port),
        CliCommand::Graph => cmd::graph::run(&common, out.as_deref()),
    }
}

fn print_usage() {
    eprintln!("usage: uhura [path] [--port <n>]");
    eprintln!("       uhura <check|export|fmt|editor|play|trace|graph> [path] [flags]");
    eprintln!(
        "       uhura export [path] --out <directory> [--mount </path/>] \
         [--play-entry </path>]"
    );
    eprintln!("       no command selects the editor (path defaults to the current directory)");
}

#[cfg(test)]
mod tests {
    use super::{CliCommand, select_command};

    #[test]
    fn no_command_defaults_to_the_editor() {
        assert_eq!(select_command(None), Ok((CliCommand::Editor, false)));
    }

    #[test]
    fn path_and_flags_are_arguments_to_the_default_editor() {
        assert_eq!(
            select_command(Some("examples/instagram/client")),
            Ok((CliCommand::Editor, false))
        );
        assert_eq!(
            select_command(Some("--port")),
            Ok((CliCommand::Editor, false))
        );
    }

    #[test]
    fn names_the_two_interactive_modes_directly() {
        assert_eq!(
            select_command(Some("editor")),
            Ok((CliCommand::Editor, true))
        );
        assert_eq!(select_command(Some("play")), Ok((CliCommand::Play, true)));
    }

    #[test]
    fn selects_static_export_as_an_explicit_command() {
        assert_eq!(
            select_command(Some("export")),
            Ok((CliCommand::Export, true))
        );
    }

    #[test]
    fn command_like_typos_still_report_as_unknown_commands() {
        assert_eq!(
            select_command(Some("definitely-not-an-uhura-command-7c449")),
            Err("definitely-not-an-uhura-command-7c449")
        );
    }
}
