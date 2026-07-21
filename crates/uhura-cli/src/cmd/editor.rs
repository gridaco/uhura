//! `uhura editor [path] [--port <n>]` — start the unified browser host with
//! the read-only Editor as the primary route.

use std::process::ExitCode;

use crate::CommonArgs;

pub fn run(common: &CommonArgs, port: u16) -> ExitCode {
    super::play::run_with_editor(common, port)
}
