//! The canonical Uhura CLI as a library. Commands share one source loader,
//! checker, machine program, evidence runner, and host.

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
