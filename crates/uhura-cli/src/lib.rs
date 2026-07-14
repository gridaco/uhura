//! The `uhura` CLI as a library: command implementations live here so the
//! gate tests (`uhura-tests`) can drive the EXACT code paths the binary
//! runs — the trace harness in particular is golden-pinned (§12.5). Only
//! this crate touches the filesystem (design §12.1).

use std::path::PathBuf;

pub mod cmd;
pub mod fsio;

#[derive(Clone)]
pub struct CommonArgs {
    pub root: PathBuf,
    pub format_json: bool,
    pub deny_warnings: bool,
    pub emit_ir: bool,
}
